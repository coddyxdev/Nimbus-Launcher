//! Modrinth API v2 client: mod/modpack search, version listing and installation.
//!
//! Only the fields the launcher actually uses are deserialised, so upstream
//! additions never break parsing.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::download::{self, DownloadTask, ExpectedHash};
use crate::error::{NimbusError, Result};

const API: &str = "https://api.modrinth.com/v2";

/// One search result row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthHit {
    #[serde(rename = "project_id")]
    pub project_id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(rename = "icon_url", default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(rename = "client_side", default)]
    pub client_side: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Vec<ModrinthHit>,
}

/// License block of a project page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthLicense {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// One screenshot of a project's gallery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthGalleryItem {
    pub url: String,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Full project page: everything the in-app details view shows, mirroring
/// what modrinth.com puts on a mod/modpack page. Field names stay snake_case
/// (no `rename_all`) so the payload matches the upstream API verbatim, like
/// the other Modrinth types here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthProject {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Long description, Markdown source as authored on Modrinth.
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub followers: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub issues_url: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub wiki_url: Option<String>,
    #[serde(default)]
    pub discord_url: Option<String>,
    #[serde(default)]
    pub client_side: Option<String>,
    #[serde(default)]
    pub server_side: Option<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub license: Option<ModrinthLicense>,
    #[serde(default)]
    pub gallery: Vec<ModrinthGalleryItem>,
}

/// A downloadable file attached to a project version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub primary: bool,
    #[serde(default, rename = "hashes")]
    pub hashes: FileHashes,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileHashes {
    #[serde(default)]
    pub sha1: Option<String>,
}

/// A single published version of a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthVersion {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Which project this version belongs to. Needed when a version is found
    /// by file hash, where the project is not known up front.
    #[serde(rename = "project_id", default)]
    pub project_id: String,
    #[serde(rename = "version_number", default)]
    pub version_number: String,
    #[serde(rename = "version_type", default)]
    pub version_type: String,
    #[serde(rename = "game_versions", default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(rename = "date_published", default)]
    pub date_published: Option<String>,
    /// Other projects this version needs (or conflicts with).
    #[serde(default)]
    pub dependencies: Vec<ModrinthDependency>,
    #[serde(default)]
    pub files: Vec<ModrinthFile>,
}

impl ModrinthVersion {
    /// The jar to install: the primary file, or the first one as a fallback.
    pub fn primary_file(&self) -> Option<&ModrinthFile> {
        self.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
    }
}

/// JSON array literal used by Modrinth facets/filters.
fn json_array(values: &[&str]) -> String {
    let inner = values
        .iter()
        .map(|v| format!("\"{v}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{inner}]")
}

async fn get_json<T: serde::de::DeserializeOwned>(
    url: &str,
    query: &[(&str, String)],
) -> Result<T> {
    let resp = download::client()
        .get(url)
        .query(query)
        .send()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(NimbusError::Http {
            status: status.as_u16(),
            url: url.to_owned(),
            retriable: status.is_server_error(),
        });
    }
    let body = resp
        .text()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;
    Ok(serde_json::from_str::<T>(&body)?)
}

/// Searches projects of a given Modrinth `project_type` ("mod", "modpack",
/// ...), optionally narrowed to a loader and Minecraft version. Shared by
/// [`search`] and [`search_modpacks`] so the facet-building logic exists only
/// once.
async fn search_typed(
    project_type: &str,
    query: &str,
    loader: Option<&str>,
    mc_version: Option<&str>,
    limit: u32,
    sort: Option<&str>,
) -> Result<Vec<ModrinthHit>> {
    // facets is an array of AND-ed groups, each group being OR-ed values.
    let mut groups: Vec<String> = vec![json_array(&[&format!("project_type:{project_type}")])];
    if let Some(loader) = loader {
        groups.push(json_array(&[&format!("categories:{loader}")]));
    }
    if let Some(mc) = mc_version {
        groups.push(json_array(&[&format!("versions:{mc}")]));
    }
    let facets = format!("[{}]", groups.join(","));

    // Only a handful of index values are valid on Modrinth; anything else
    // falls back to relevance instead of erroring the whole search.
    let index = match sort {
        Some("downloads") => "downloads",
        Some("follows") => "follows",
        Some("newest") => "newest",
        Some("updated") => "updated",
        _ => "relevance",
    };

    let params = vec![
        ("query", query.to_owned()),
        ("limit", limit.clamp(1, 50).to_string()),
        ("index", index.to_owned()),
        ("facets", facets),
    ];
    let resp: SearchResponse = get_json(&format!("{API}/search"), &params).await?;
    Ok(resp.hits)
}

/// Searches mods, optionally narrowed to a loader and Minecraft version.
pub async fn search(
    query: &str,
    loader: Option<&str>,
    mc_version: Option<&str>,
    limit: u32,
    sort: Option<&str>,
) -> Result<Vec<ModrinthHit>> {
    search_typed("mod", query, loader, mc_version, limit, sort).await
}

/// Searches modpacks, optionally narrowed to a loader and Minecraft version.
pub async fn search_modpacks(
    query: &str,
    loader: Option<&str>,
    mc_version: Option<&str>,
    limit: u32,
    sort: Option<&str>,
) -> Result<Vec<ModrinthHit>> {
    search_typed("modpack", query, loader, mc_version, limit, sort).await
}

/// Lists versions of a project compatible with the given loader / MC version.
pub async fn versions(
    project_id: &str,
    loader: Option<&str>,
    mc_version: Option<&str>,
) -> Result<Vec<ModrinthVersion>> {
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(loader) = loader {
        params.push(("loaders", json_array(&[loader])));
    }
    if let Some(mc) = mc_version {
        params.push(("game_versions", json_array(&[mc])));
    }
    let url = format!("{API}/project/{project_id}/version");
    get_json(&url, &params).await
}

/// Fetches the full project page (long description, gallery, links, stats).
pub async fn project(project_id: &str) -> Result<ModrinthProject> {
    let url = format!("{API}/project/{project_id}");
    get_json(&url, &[]).await
}

/// Fetches a single version by id, regardless of project.
pub async fn version_by_id(version_id: &str) -> Result<ModrinthVersion> {
    let url = format!("{API}/version/{version_id}");
    get_json(&url, &[]).await
}

/// Picks the newest compatible version for a project.
pub async fn best_version(
    project_id: &str,
    loader: Option<&str>,
    mc_version: Option<&str>,
) -> Result<ModrinthVersion> {
    let list = versions(project_id, loader, mc_version).await?;
    // Modrinth returns newest first; prefer a release over beta/alpha when the
    // newest entry is a pre-release.
    let chosen = list
        .iter()
        .find(|v| v.version_type == "release")
        .or_else(|| list.first())
        .cloned();
    chosen.ok_or_else(|| {
        NimbusError::Invalid("Для этой версии игры и загрузчика мод не найден".to_owned())
    })
}

/// Downloads the version's primary jar into `mods_dir` and returns its name.
pub async fn install_version(mods_dir: &Path, version: &ModrinthVersion) -> Result<String> {
    let file = version.primary_file().ok_or_else(|| {
        NimbusError::Invalid("У версии мода нет файлов для скачивания".to_owned())
    })?;

    if !file.filename.ends_with(".jar") {
        return Err(NimbusError::Invalid(format!(
            "Неожиданный тип файла: {}",
            file.filename
        )));
    }

    tokio::fs::create_dir_all(mods_dir).await?;
    let dest = mods_dir.join(&file.filename);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move { while rx.recv().await.is_some() {} });

    download::download_one(
        DownloadTask {
            url: file.url.clone(),
            dest,
            hash: file.hashes.sha1.clone().map(ExpectedHash::Sha1),
            size: if file.size > 0 { Some(file.size) } else { None },
        },
        tx,
    )
    .await?;

    Ok(file.filename.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_array_quotes_every_value() {
        assert_eq!(json_array(&["a"]), "[\"a\"]");
        assert_eq!(json_array(&["a", "b"]), "[\"a\",\"b\"]");
    }

    #[test]
    fn primary_file_prefers_the_primary_flag() {
        let mk = |name: &str, primary: bool| ModrinthFile {
            url: "u".into(),
            filename: name.into(),
            size: 1,
            primary,
            hashes: FileHashes::default(),
        };
        let v = ModrinthVersion {
            id: "1".into(),
            name: String::new(),
            version_number: String::new(),
            version_type: "release".into(),
            game_versions: vec![],
            loaders: vec![],
            date_published: None,
            project_id: String::new(),
            dependencies: vec![],
            files: vec![mk("sources.jar", false), mk("mod.jar", true)],
        };
        assert_eq!(v.primary_file().unwrap().filename, "mod.jar");
    }
}

// ─── Dependencies and hash lookup ───────────────────────────────────────

/// One dependency declared by a version. Field names stay snake_case, like
/// [`ModrinthProject`], so the payload matches the upstream API verbatim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthDependency {
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    /// `required` | `optional` | `incompatible` | `embedded`.
    #[serde(default)]
    pub dependency_type: String,
}

/// Looks up which Modrinth version each SHA-1 belongs to.
///
/// This is how installed jars are identified: the launcher never stores mod
/// metadata itself, it just hashes the files on disk and asks Modrinth. Files
/// that are not on Modrinth (hand-made jars) are simply missing from the map.
pub async fn versions_by_hashes(
    hashes: &[String],
) -> Result<std::collections::HashMap<String, ModrinthVersion>> {
    if hashes.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let url = format!("{API}/version_files");
    let body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1" });

    let resp = download::client()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(NimbusError::Http {
            status: status.as_u16(),
            url,
            retriable: status.is_server_error(),
        });
    }

    let text = resp
        .text()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;
    Ok(serde_json::from_str(&text)?)
}

/// Downloads a version's primary file into `dest_dir` without assuming it is a
/// jar. Used for resource packs, shaders and data packs, where the extension
/// is `.zip`.
pub async fn install_version_file(dest_dir: &Path, version: &ModrinthVersion) -> Result<String> {
    let file = version.primary_file().ok_or_else(|| {
        NimbusError::Invalid("У этой версии нет файлов для скачивания".to_owned())
    })?;

    // Guard against a crafted filename escaping the target directory.
    if file.filename.contains('/')
        || file.filename.contains('\\')
        || file.filename.contains("..")
    {
        return Err(NimbusError::Invalid(format!(
            "Недопустимое имя файла: {}",
            file.filename
        )));
    }

    tokio::fs::create_dir_all(dest_dir).await?;
    let dest = dest_dir.join(&file.filename);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move { while rx.recv().await.is_some() {} });

    download::download_one(
        DownloadTask {
            url: file.url.clone(),
            dest,
            hash: file.hashes.sha1.clone().map(ExpectedHash::Sha1),
            size: Some(file.size).filter(|s| *s > 0),
        },
        tx,
    )
    .await?;

    Ok(file.filename.clone())
}
