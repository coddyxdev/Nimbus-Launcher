//! Minecraft version manifest and version JSON.
//!
//! `version_manifest_v2.json` is cached to disk with ETag / Last-Modified
//! round-trip validation; the launcher works from the cached copy when the
//! network is unavailable. Version JSONs are cached similarly. The
//! `inheritsFrom` chain is resolved recursively before being returned.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::download::client;
use crate::error::{NimbusError, Result};
use crate::paths;

const MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const MANIFEST_CACHE_DEPTH: u32 = 32; // guard against circular inheritsFrom

// ─── Manifest ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ManifestLatest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub time: String,
    pub release_time: String,
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    #[allow(dead_code)]
    pub latest: ManifestLatest,
    pub versions: Vec<ManifestEntry>,
}

/// Small sidecar stored next to cached JSON files so we can do conditional GETs.
#[derive(Debug, Default, Deserialize, Serialize)]
struct CacheHeaders {
    etag: Option<String>,
    last_modified: Option<String>,
}

// ─── Version JSON types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionDownload {
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct VersionDownloads {
    pub client: VersionDownload,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    pub id: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
    pub total_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: Option<String>,
    pub major_version: u32,
}

/// A rule condition: os, features, or both.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuleOs {
    pub name: Option<String>,
    pub version: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuleFeatures {
    pub is_demo_user: Option<bool>,
    pub has_custom_resolution: Option<bool>,
    pub has_quick_plays_support: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<RuleOs>,
    pub features: Option<RuleFeatures>,
}

/// A single argument element: either a bare string or a conditional object.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ArgElement {
    Bare(String),
    Conditional { rules: Vec<Rule>, value: ArgValue },
}

/// `value` can be a single string or an array.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

impl ArgValue {
    #[allow(dead_code)]
    pub fn as_slice(&self) -> &[String] {
        match self {
            Self::One(s) => std::slice::from_ref(s),
            Self::Many(v) => v,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<ArgElement>,
    #[serde(default)]
    pub jvm: Vec<ArgElement>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryNatives {
    pub windows: Option<String>,
    pub linux: Option<String>,
    pub osx: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LibraryDownloadArtifact {
    pub path: Option<String>,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LibraryDownloadClassifiers {
    #[serde(flatten)]
    pub entries: std::collections::HashMap<String, LibraryDownloadArtifact>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LibraryDownloads {
    pub artifact: Option<LibraryDownloadArtifact>,
    pub classifiers: Option<LibraryDownloadClassifiers>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LibraryExtract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LibraryJson {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<LibraryNatives>,
    pub extract: Option<LibraryExtract>,
    pub url: Option<String>,
}

/// The fully-merged version profile. All `Option` fields may be absent in
/// older or mod-loader child profiles; they are filled during `inheritsFrom`
/// resolution by copying from the parent.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionMeta {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub release_time: Option<String>,
    pub downloads: Option<VersionDownloads>,
    pub asset_index: Option<AssetIndex>,
    pub assets: Option<String>,
    pub java_version: Option<JavaVersion>,
    pub libraries: Vec<LibraryJson>,
    pub main_class: Option<String>,
    /// New-format arguments (≥ 1.13).
    pub arguments: Option<Arguments>,
    /// Old-format game arguments (< 1.13).
    pub minecraft_arguments: Option<String>,
    /// If present this profile extends the named version.
    pub inherits_from: Option<String>,
    /// Passed to the log4j2 formatter flag logic.
    pub minimum_launcher_version: Option<u32>,
    /// Logging configuration (we only expose it; the flag logic uses release_time).
    pub logging: Option<serde_json::Value>,
}

// ─── Cache helpers ────────────────────────────────────────────────────────────

fn versions_cache_dir() -> Result<PathBuf> {
    Ok(paths::shared_dir()?.join("versions"))
}

fn manifest_cache_path() -> Result<PathBuf> {
    Ok(versions_cache_dir()?.join("version_manifest_v2.json"))
}

fn manifest_headers_path() -> Result<PathBuf> {
    Ok(versions_cache_dir()?.join("version_manifest_v2.headers.json"))
}

fn version_cache_path(id: &str) -> Result<PathBuf> {
    Ok(versions_cache_dir()?.join(id).join(format!("{id}.json")))
}

fn version_headers_path(id: &str) -> Result<PathBuf> {
    Ok(versions_cache_dir()?
        .join(id)
        .join(format!("{id}.headers.json")))
}

fn read_headers(path: &Path) -> CacheHeaders {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_headers(path: &Path, headers: &CacheHeaders) {
    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            crate::nlog!("version cache: failed to create {parent:?} ({err})");
            return;
        }
    }
    match serde_json::to_vec_pretty(headers) {
        Ok(json) => {
            if let Err(err) = crate::config::write_atomic(path, &json) {
                crate::nlog!("version cache: failed to write headers to {path:?} ({err})");
            }
        }
        Err(err) => crate::nlog!("version cache: failed to serialise headers ({err})"),
    }
}

/// Performs a conditional GET, updating the cache file when the server returns
/// 200 and reusing the cached copy on 304. Falls back to the cached copy on
/// any network error if it exists.
async fn conditional_get(url: &str, cache: &Path, headers_path: &Path) -> Result<String> {
    if let Some(parent) = cache.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let stored = read_headers(headers_path);
    let mut req = client().get(url);
    if let Some(etag) = &stored.etag {
        req = req.header("If-None-Match", etag);
    }
    if let Some(lm) = &stored.last_modified {
        req = req.header("If-Modified-Since", lm);
    }

    match req.send().await {
        Ok(resp) if resp.status() == reqwest::StatusCode::NOT_MODIFIED => {
            // Cache is current.
            return tokio::fs::read_to_string(cache)
                .await
                .map_err(NimbusError::from);
        }
        Ok(resp) if resp.status().is_success() => {
            let new_etag = resp
                .headers()
                .get("ETag")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let new_lm = resp
                .headers()
                .get("Last-Modified")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let body = resp.text().await?;
            crate::config::write_atomic(cache, body.as_bytes())?;
            write_headers(
                headers_path,
                &CacheHeaders {
                    etag: new_etag,
                    last_modified: new_lm,
                },
            );
            Ok(body)
        }
        Ok(resp) => {
            // Non-success, non-304: use cached copy if available.
            if cache.exists() {
                return tokio::fs::read_to_string(cache).await.map_err(Into::into);
            }
            Err(NimbusError::Http {
                status: resp.status().as_u16(),
                url: url.to_owned(),
                retriable: resp.status().is_server_error(),
            })
        }
        Err(_) if cache.exists() => {
            // Network is down: serve from cache.
            tokio::fs::read_to_string(cache).await.map_err(Into::into)
        }
        Err(err) => Err(NimbusError::Network(err.to_string())),
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Summary returned to the frontend version list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub release_time: String,
}

pub async fn list_versions(include_snapshots: bool) -> Result<Vec<VersionSummary>> {
    let cache = manifest_cache_path()?;
    let headers = manifest_headers_path()?;
    let body = conditional_get(MANIFEST_URL, &cache, &headers).await?;
    let manifest: Manifest = serde_json::from_str(&body)?;

    let summaries = manifest
        .versions
        .into_iter()
        .filter(|v| {
            include_snapshots || matches!(v.kind.as_str(), "release" | "old_beta" | "old_alpha")
        })
        .map(|v| VersionSummary {
            id: v.id,
            kind: v.kind,
            release_time: v.release_time,
        })
        .collect();
    Ok(summaries)
}

/// Fetches and caches the raw version JSON for `id` without resolving
/// `inheritsFrom`. Used internally by `fetch_version_meta`.
async fn fetch_raw(manifest: &Manifest, id: &str) -> Result<VersionMeta> {
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == id)
        .ok_or_else(|| NimbusError::VersionNotFound(id.to_owned()))?;

    let cache = version_cache_path(id)?;
    let headers = version_headers_path(id)?;
    let body = conditional_get(&entry.url, &cache, &headers).await?;
    Ok(serde_json::from_str(&body)?)
}

/// Loads the manifest (from cache or network).
pub async fn load_manifest() -> Result<Manifest> {
    let cache = manifest_cache_path()?;
    let headers = manifest_headers_path()?;
    let body = conditional_get(MANIFEST_URL, &cache, &headers).await?;
    Ok(serde_json::from_str(&body)?)
}

/// Save a non-Mojang profile (e.g. Fabric loader JSON) to the versions cache.
/// The profile is stored as a version JSON at `shared/versions/<id>/<id>.json`
/// so that `fetch_any_version` can find it during inheritsFrom resolution.
pub fn cache_custom_profile(id: &str, json: &str) -> Result<()> {
    let path = version_cache_path(id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::config::write_atomic(&path, json.as_bytes())
}

/// Try to load a version from the local filesystem cache without any network
/// request. Returns `None` if the cache file does not exist.
pub(crate) async fn fetch_cached_profile(id: &str) -> Result<Option<VersionMeta>> {
    let path = version_cache_path(id)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = tokio::fs::read_to_string(&path).await?;
    Ok(Some(serde_json::from_str(&raw)?))
}

/// Try to load from: 1) custom/cached profile, 2) Mojang manifest.
/// Used to resolve versions that may not be in the Mojang manifest
/// (e.g. Fabric loader profiles).
async fn fetch_any_raw(manifest: &Manifest, id: &str) -> Result<VersionMeta> {
    // First, check local cache (custom profiles like Fabric/Forge)
    if let Some(profile) = fetch_cached_profile(id).await? {
        return Ok(profile);
    }
    // Fall through to Mojang manifest
    fetch_raw(manifest, id).await
}

/// Resolves any version profile, following `inheritsFrom`.
/// Unlike `fetch_version_meta`, this also checks cached custom profiles
/// (Fabric, Forge, etc.) that are not in the Mojang manifest.
pub async fn fetch_any_version(id: &str) -> Result<VersionMeta> {
    let manifest = load_manifest().await?;
    let mut current = fetch_any_raw(&manifest, id).await?;
    let mut depth = 0u32;

    while let Some(parent_id) = current.inherits_from.take() {
        depth += 1;
        if depth > MANIFEST_CACHE_DEPTH {
            return Err(NimbusError::Invalid(format!(
                "inheritsFrom chain for '{id}' exceeds {MANIFEST_CACHE_DEPTH} hops"
            )));
        }
        let parent = fetch_any_raw(&manifest, &parent_id).await?;
        current = merge(current, parent);
    }

    Ok(current)
}

/// Merges `child` on top of `parent`.
///
/// Rules:
/// - `libraries`: child first, then parent (child overrides same coordinate).
/// - `arguments.game` / `arguments.jvm`: child first, then parent.
/// - `minecraftArguments`: child wins if present, else parent.
/// - All other fields: child wins if present, else parent.
fn merge(mut child: VersionMeta, parent: VersionMeta) -> VersionMeta {
    // Libraries: append parent's libraries whose groupId:artifactId is not
    // already present in child (child version wins regardless of number).
    fn ga(name: &str) -> &str {
        // "com.google.guava:guava:21.0" → "com.google.guava:guava"
        match name.match_indices(':').nth(1) {
            Some((idx, _)) => &name[..idx],
            None => name,
        }
    }
    let child_gas: std::collections::HashSet<&str> =
        child.libraries.iter().map(|l| ga(&l.name)).collect();
    let extra: Vec<LibraryJson> = parent
        .libraries
        .into_iter()
        .filter(|l| !child_gas.contains(ga(&l.name)))
        .collect();
    child.libraries.extend(extra);

    // Arguments: child first, then parent.
    if let Some(parent_args) = parent.arguments {
        let child_args = child.arguments.get_or_insert_with(Arguments::default);
        // Prepend parent args that are bare strings not already in child.
        // For unconditional args we concatenate; order: child then parent.
        child_args.game.extend(parent_args.game);
        child_args.jvm.extend(parent_args.jvm);
    }

    // minecraftArguments: child wins.
    if child.minecraft_arguments.is_none() {
        child.minecraft_arguments = parent.minecraft_arguments;
    }

    // Scalar fields: fill from parent if child doesn't have them.
    if child.downloads.is_none() {
        child.downloads = parent.downloads;
    }
    if child.asset_index.is_none() {
        child.asset_index = parent.asset_index;
    }
    if child.assets.is_none() {
        child.assets = parent.assets;
    }
    if child.java_version.is_none() {
        child.java_version = parent.java_version;
    }
    if child.main_class.is_none() {
        child.main_class = parent.main_class;
    }
    if child.kind.is_none() {
        child.kind = parent.kind;
    }
    if child.release_time.is_none() {
        child.release_time = parent.release_time;
    }
    if child.logging.is_none() {
        child.logging = parent.logging;
    }
    child
}

/// Fully resolves a version profile, following the `inheritsFrom` chain.
/// Stops after `MANIFEST_CACHE_DEPTH` hops to prevent infinite loops in
/// malformed mod-loader profiles.
pub async fn fetch_version_meta(id: &str) -> Result<VersionMeta> {
    let manifest = load_manifest().await?;
    let mut current = fetch_raw(&manifest, id).await?;
    let mut depth = 0u32;

    while let Some(parent_id) = current.inherits_from.take() {
        depth += 1;
        if depth > MANIFEST_CACHE_DEPTH {
            return Err(NimbusError::Invalid(format!(
                "inheritsFrom chain for '{id}' exceeds {MANIFEST_CACHE_DEPTH} hops"
            )));
        }
        let parent = fetch_raw(&manifest, &parent_id).await?;
        current = merge(current, parent);
    }

    Ok(current)
}

/// Returns the path where the client jar should live in the shared store.
pub fn client_jar_path(version_id: &str) -> Result<PathBuf> {
    Ok(versions_cache_dir()?
        .join(version_id)
        .join(format!("{version_id}.jar")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_meta(id: &str, libs: &[&str]) -> VersionMeta {
        VersionMeta {
            id: id.to_owned(),
            libraries: libs
                .iter()
                .map(|n| LibraryJson {
                    name: n.to_string(),
                    downloads: None,
                    rules: None,
                    natives: None,
                    extract: None,
                    url: None,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn merge_deduplicates_libraries_keeping_child() {
        let child = make_meta(
            "fabric",
            &[
                "net.fabricmc:fabric-loader:0.15",
                "com.google.guava:guava:21.0",
            ],
        );
        let parent = make_meta(
            "1.20.1",
            &["com.google.guava:guava:17.0", "org.ow2.asm:asm:9.6"],
        );
        let merged = merge(child, parent);
        // guava from child (21.0) should survive; parent's (17.0) is dropped.
        let guava_count = merged
            .libraries
            .iter()
            .filter(|l| l.name.starts_with("com.google.guava:guava"))
            .count();
        assert_eq!(guava_count, 1);
        assert!(merged
            .libraries
            .iter()
            .any(|l| l.name == "com.google.guava:guava:21.0"));
        // asm from parent should be added.
        assert!(merged
            .libraries
            .iter()
            .any(|l| l.name.starts_with("org.ow2.asm:asm")));
    }

    #[test]
    fn merge_fills_missing_scalar_from_parent() {
        let mut child = make_meta("child", &[]);
        let mut parent = make_meta("parent", &[]);
        parent.main_class = Some("net.minecraft.client.main.Main".to_owned());
        parent.release_time = Some("2024-01-01T00:00:00+00:00".to_owned());
        child = merge(child, parent);
        assert_eq!(
            child.main_class.as_deref(),
            Some("net.minecraft.client.main.Main")
        );
    }
}
