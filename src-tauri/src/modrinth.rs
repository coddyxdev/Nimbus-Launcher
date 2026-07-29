//! Modrinth API v2 client: mod search, version listing and installation.
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

/// Searches mods, optionally narrowed to a loader and Minecraft version.
pub async fn search(
    query: &str,
    loader: Option<&str>,
    mc_version: Option<&str>,
    limit: u32,
) -> Result<Vec<ModrinthHit>> {
    // facets is an array of AND-ed groups, each group being OR-ed values.
    let mut groups: Vec<String> = vec![json_array(&["project_type:mod"])];
    if let Some(loader) = loader {
        groups.push(json_array(&[&format!("categories:{loader}")]));
    }
    if let Some(mc) = mc_version {
        groups.push(json_array(&[&format!("versions:{mc}")]));
    }
    let facets = format!("[{}]", groups.join(","));

    let params = vec![
        ("query", query.to_owned()),
        ("limit", limit.clamp(1, 50).to_string()),
        ("index", "relevance".to_owned()),
        ("facets", facets),
    ];
    let resp: SearchResponse = get_json(&format!("{API}/search"), &params).await?;
    Ok(resp.hits)
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
            files: vec![mk("sources.jar", false), mk("mod.jar", true)],
        };
        assert_eq!(v.primary_file().unwrap().filename, "mod.jar");
    }
}
