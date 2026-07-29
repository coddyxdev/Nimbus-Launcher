//! Mod loader integration: Fabric, Quilt, Forge, NeoForge.
//!
//! Each loader has a metadata API that returns a version profile JSON structurally
//! identical to a Mojang version JSON. These profiles use `inheritsFrom` to extend
//! a base Minecraft version, so the existing `VersionMeta` resolution in
//! `version.rs` handles merging automatically once the profile is cached.
//!
//! Fabric and Quilt share an identical API pattern (Meta REST API → profile JSON).
//! Forge uses Maven metadata for version listing and wraps profile JSONs inside
//! installer jars. NeoForge has a similar Maven-based scheme.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::download::{client, DownloadTask, ProgressEvent, download_one};
use crate::error::{NimbusError, Result};
use crate::version;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Supported mod loaders.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModLoader {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl ModLoader {
    /// Canonical short name used in profile IDs and instance metadata.
    pub fn as_str(&self) -> &'static str {
        match self {
            ModLoader::Fabric => "fabric",
            ModLoader::Quilt => "quilt",
            ModLoader::Forge => "forge",
            ModLoader::NeoForge => "neoforge",
        }
    }

    /// Parse from canonical string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "fabric" => Some(ModLoader::Fabric),
            "quilt" => Some(ModLoader::Quilt),
            "forge" => Some(ModLoader::Forge),
            "neoforge" => Some(ModLoader::NeoForge),
            _ => None,
        }
    }

    /// Display name for the frontend.
    pub fn display_name(&self) -> &'static str {
        match self {
            ModLoader::Fabric => "Fabric",
            ModLoader::Quilt => "Quilt",
            ModLoader::Forge => "Forge",
            ModLoader::NeoForge => "NeoForge",
        }
    }

    /// All loaders for frontend listing.
    pub fn all() -> &'static [ModLoader] {
        &[ModLoader::Fabric, ModLoader::Quilt, ModLoader::Forge, ModLoader::NeoForge]
    }
}

/// A single available loader version returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersionInfo {
    pub version: String,
    pub stable: bool,
}

/// Builds the canonical profile ID used to cache loader version JSONs.
/// Example: `fabric-loader-0.16.0-1.21`
pub fn profile_id(loader: &ModLoader, loader_version: &str, mc_version: &str) -> String {
    format!("{}-loader-{}-{}", loader.as_str(), loader_version, mc_version)
}

// ─── Fabric / Quilt (identical Meta API) ──────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FabricLoaderEntry {
    loader: FabricLoaderMeta,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FabricLoaderMeta {
    version: String,
    /// Quilt API doesn't return `stable`, only Fabric does.
    /// Default to `false` when absent.
    #[serde(default)]
    stable: bool,
}

async fn list_meta_versions(base_url: &str, mc_version: &str) -> Result<Vec<LoaderVersionInfo>> {
    let url = format!("{}/versions/loader/{}", base_url, mc_version);
    let resp = client().get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(NimbusError::Http {
            status: resp.status().as_u16(),
            url,
            retriable: resp.status().is_server_error(),
        });
    }
    let entries: Vec<FabricLoaderEntry> = resp.json().await?;
    Ok(entries
        .into_iter()
        .map(|e| LoaderVersionInfo {
            version: e.loader.version,
            stable: e.loader.stable,
        })
        .collect())
}

async fn download_meta_profile(
    base_url: &str,
    mc_version: &str,
    loader_version: &str,
    loader: &ModLoader,
    progress: Option<crate::download::ProgressSender>,
) -> Result<()> {
    let url = format!(
        "{}/versions/loader/{}/{}/profile/json",
        base_url, mc_version, loader_version
    );
    let resp = client().get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(NimbusError::Http {
            status: resp.status().as_u16(),
            url,
            retriable: resp.status().is_server_error(),
        });
    }
    let body = resp.text().await?;
    let id = profile_id(loader, loader_version, mc_version);
    version::cache_custom_profile(&id, &body)?;
    if let Some(ref tx) = progress {
        let _ = tx.send(ProgressEvent::Finished {
            file: format!("{id}.json"),
        });
    }
    Ok(())
}

// ─── Forge ────────────────────────────────────────────────────────────────────

/// Forge version strings look like `1.20.1-47.1.30` where the prefix before
/// the first `-` is the Minecraft version.
#[allow(dead_code)]
fn forge_mc_version(forge_ver: &str) -> Option<String> {
    forge_ver.split('-').next().map(|s| s.to_owned())
}

/// Parses the `<latest>` and `<release>` values from a Maven metadata XML.
fn parse_maven_release_xml(xml: &str) -> Option<(String, String)> {
    let mut latest = None;
    let mut release = None;
    for line in xml.lines() {
        let t = line.trim();
        if let Some(inner) = t.strip_prefix("<latest>") {
            if let Some(ver) = inner.strip_suffix("</latest>") {
                latest = Some(ver.to_owned());
            }
        }
        if let Some(inner) = t.strip_prefix("<release>") {
            if let Some(ver) = inner.strip_suffix("</release>") {
                release = Some(ver.to_owned());
            }
        }
    }
    match (latest, release) {
        (Some(l), Some(r)) => Some((l, r)),
        _ => None,
    }
}

async fn list_forge_versions(mc_version: &str) -> Result<Vec<LoaderVersionInfo>> {
    // Forge only serves maven-metadata.xml (JSON variant returns 404)
    let url = "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
    let resp = client().get(url).send().await?;
    if !resp.status().is_success() {
        return Err(NimbusError::Http {
            status: resp.status().as_u16(),
            url: url.to_owned(),
            retriable: resp.status().is_server_error(),
        });
    }
    let xml = resp.text().await?;
    let all_versions = parse_maven_metadata_xml(&xml);
    let (_, release) = parse_maven_release_xml(&xml).unwrap_or_default();

    // Forge 1.13+ versions: <mc_version>-<forge_build>.
    // Use exact segment comparison (split on "-") instead of starts_with
    // to avoid false matches like "1.21.1-" matching "1.21.11-61.1.14" -> 404.

    Ok(all_versions
        .into_iter()
        .filter(|v| v.split("-").next() == Some(mc_version))
        .map(|v| {
            // Extract just the Forge build version (after the first "-")
            let build_ver = v.split("-").nth(1).unwrap_or(&v).to_owned();
            let full_release = format!("{}-{}", mc_version, release);
            LoaderVersionInfo {
                version: build_ver,
                stable: v == full_release,
            }
        })
        .collect())
}

async fn download_forge_profile(
    mc_version: &str,
    loader_version: &str,
    progress: Option<crate::download::ProgressSender>,
) -> Result<()> {
    let loader = ModLoader::Forge;
    let forge_full_version = format!("{}-{}", mc_version, loader_version);

    // Forge stopped serving -client.json in Maven. Always fall through to the
    // installer jar and extract version.json from it.
    // The installer jar URL pattern: forge-<fullVersion>-installer.jar
    let installer_url = format!(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/{fv}/forge-{fv}-installer.jar",
        fv = forge_full_version
    );

    // Lightweight HEAD check: verify the installer URL exists before downloading.
    // Only fetch full metadata (5000 lines) if the URL returns 404.
    // HEAD errors (405 Method Not Allowed, network issues) are NON-FATAL -
    // we fall through to the actual download which will report 404 correctly.
    let head_result = client().head(&installer_url).send().await;
    if let Ok(head_resp) = head_result {
        let status = head_resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            let available = list_forge_versions(mc_version).await?;
            let versions: Vec<&str> = available.iter().map(|v| v.version.as_str()).collect();
            return Err(NimbusError::Invalid(format!(
                "Версия Forge {} не существует для Minecraft {}. Доступные: {}",
                loader_version,
                mc_version,
                if versions.is_empty() {
                    "нет".to_string()
                } else {
                    versions.join(", ")
                }
            )));
        }
        // 405/403 means HEAD not supported — skip validation, proceed to GET
        if !status.is_success()
            && status != reqwest::StatusCode::METHOD_NOT_ALLOWED
            && status != reqwest::StatusCode::FORBIDDEN
        {
            return Err(NimbusError::Http {
                status: status.as_u16(),
                url: installer_url.clone(),
                retriable: status.is_server_error(),
            });
        }
    }
    // HEAD failed (network error) or returned 405/403 — proceed to actual download
    let tmp_dir = std::env::temp_dir().join("nimbus-forge-installers");
    tokio::fs::create_dir_all(&tmp_dir).await?;

    let installer_jar = tmp_dir.join(format!("forge-{fv}-installer.jar", fv = forge_full_version));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ProgressEvent>();
    // Drain progress
    tokio::spawn(async move {
        while rx.recv().await.is_some() {}
    });

    download_one(
        DownloadTask {
            url: installer_url,
            dest: installer_jar.clone(),
            hash: None,
            size: None,
        },
        tx,
    )
    .await?;

    // Extract version.json from the installer jar
    let file = std::fs::File::open(&installer_jar)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| NimbusError::Zip(e.to_string()))?;

    let mut found = false;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| NimbusError::Zip(e.to_string()))?;
        if entry.name() == "version.json" {
            let mut body = String::new();
            std::io::Read::read_to_string(&mut entry, &mut body)?;
            let id = profile_id(&loader, loader_version, mc_version);
            version::cache_custom_profile(&id, &body)?;
            found = true;
            if let Some(ref tx) = progress {
                let _ = tx.send(ProgressEvent::Finished {
                    file: format!("{id}.json"),
                });
            }
            break;
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&installer_jar);

    if !found {
        return Err(NimbusError::Invalid(format!(
            "Forge installer for {forge_full_version} does not contain version.json"
        )));
    }

    Ok(())
}

// ─── NeoForge ─────────────────────────────────────────────────────────────────

/// Parses Maven-metadata.xml for NeoForge. The format is very simple:
/// ```xml
/// <metadata><versioning><versions><version>x.y.z</version>...</versions></versioning></metadata>
/// ```
/// We parse it without an XML crate since the structure is fixed.
fn parse_maven_metadata_xml(xml: &str) -> Vec<String> {
    let mut versions = Vec::new();
    let mut in_versions = false;

    for line in xml.lines() {
        let t = line.trim();
        if t == "<versions>" {
            in_versions = true;
            continue;
        }
        if t == "</versions>" {
            break;
        }
        if in_versions {
            if let Some(inner) = t.strip_prefix("<version>") {
                if let Some(ver) = inner.strip_suffix("</version>") {
                    versions.push(ver.to_owned());
                }
            }
        }
    }
    versions
}

/// NeoForge version → Minecraft version mapping.
///
/// The official version scheme: NeoForge `X.Y.Z` targets Minecraft `1.X.Y`
/// (where X is the MC major, Y the MC minor, Z the NeoForge patch).
/// Examples:
/// - NeoForge 21.0.x → MC 1.21
/// - NeoForge 20.4.x → MC 1.20.4
/// - NeoForge 20.2.x → MC 1.20.2
fn neoforge_to_mc_prefix(neoforge_ver: &str) -> Option<String> {
    let parts: Vec<&str> = neoforge_ver.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    // The first two components of NeoForge version are the MC version numbers
    // (without the "1." prefix). Strip trailing ".0" so that:
    // "1.21.0" → "1.21"  (matches mc_version "1.21")
    // "1.20.4" stays     (matches mc_version "1.20.4")
    let result = format!("1.{}.{}", parts[0], parts[1]);
    Some(result.trim_end_matches(".0").to_string())
}

async fn list_neoforge_versions(mc_version: &str) -> Result<Vec<LoaderVersionInfo>> {
    let url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
    let resp = client().get(url).send().await?;
    if !resp.status().is_success() {
        return Err(NimbusError::Http {
            status: resp.status().as_u16(),
            url: url.to_owned(),
            retriable: resp.status().is_server_error(),
        });
    }
    let xml = resp.text().await?;
    let all_versions = parse_maven_metadata_xml(&xml);

    // Find the latest version for each MC minor matching our target
    let mut matched: Vec<LoaderVersionInfo> = all_versions
        .iter()
        .filter(|v| {
            neoforge_to_mc_prefix(v)
                .map(|prefix| prefix == mc_version)
                .unwrap_or(false)
        })
        .map(|v| LoaderVersionInfo {
            version: v.clone(),
            stable: true, // NeoForge doesn't distinguish stable/beta per-entry
        })
        .collect();

    // Sort by version (newest first)
    matched.sort_by(|a, b| {
        let a_parts: Vec<u64> = a.version.split('.').filter_map(|s| s.parse().ok()).collect();
        let b_parts: Vec<u64> = b.version.split('.').filter_map(|s| s.parse().ok()).collect();
        for (ap, bp) in a_parts.iter().zip(b_parts.iter()) {
            if ap != bp {
                return bp.cmp(ap);
            }
        }
        b_parts.len().cmp(&a_parts.len())
    });

    Ok(matched)
}

async fn download_neoforge_profile(
    mc_version: &str,
    loader_version: &str,
    progress: Option<crate::download::ProgressSender>,
) -> Result<()> {
    let loader = ModLoader::NeoForge;

    // Try profile JSON first (modern NeoForge)
    let url = format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{lv}/neoforge-{lv}-client.json",
        lv = loader_version
    );

    let resp = client().get(&url).send().await;
    match resp {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await?;
            let id = profile_id(&loader, loader_version, mc_version);
            version::cache_custom_profile(&id, &body)?;
            if let Some(ref tx) = progress {
                let _ = tx.send(ProgressEvent::Finished {
                    file: format!("{id}.json"),
                });
            }
            return Ok(());
        }
        Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
            // Profile JSON not available; try extracting from installer jar
            // (NeoForge may use installer jars similar to Forge)
        }
        Ok(resp) => {
            return Err(NimbusError::Http {
                status: resp.status().as_u16(),
                url,
                retriable: resp.status().is_server_error(),
            });
        }
        Err(e) => {
            return Err(NimbusError::Network(e.to_string()));
        }
    }

    // Fallback: try downloading the installer jar and extracting version.json
    let installer_url = format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{lv}/neoforge-{lv}-installer.jar",
        lv = loader_version
    );

    let tmp_dir = std::env::temp_dir().join("nimbus-neoforge-installers");
    tokio::fs::create_dir_all(&tmp_dir).await?;
    let installer_jar = tmp_dir.join(format!("neoforge-{lv}-installer.jar", lv = loader_version));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ProgressEvent>();
    tokio::spawn(async move { while rx.recv().await.is_some() {} });

    download_one(
        DownloadTask {
            url: installer_url,
            dest: installer_jar.clone(),
            hash: None,
            size: None,
        },
        tx,
    )
    .await?;

    let file = std::fs::File::open(&installer_jar)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| NimbusError::Zip(e.to_string()))?;

    let mut found = false;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| NimbusError::Zip(e.to_string()))?;
        let entry_name = entry.name().to_owned();
        // NeoForge stores the profile as either `version.json` or `<name>.json`
        if entry_name == "version.json" || entry_name.ends_with("-client.json") {
            let mut body = String::new();
            std::io::Read::read_to_string(&mut entry, &mut body)?;
            let id = profile_id(&loader, loader_version, mc_version);
            version::cache_custom_profile(&id, &body)?;
            found = true;
            if let Some(ref tx) = progress {
                let _ = tx.send(ProgressEvent::Finished {
                    file: format!("{id}.json"),
                });
            }
            break;
        }
    }

    let _ = std::fs::remove_file(&installer_jar);

    if !found {
        return Err(NimbusError::Invalid(format!(
            "NeoForge {loader_version} profile could not be downloaded for MC {mc_version}"
        )));
    }

    Ok(())
}

// ─── Fabric API (Modrinth) ─────────────────────────────────────────────────────

/// Downloads the latest Fabric API jar for the given MC version.
/// Returns the path to the downloaded jar.
pub async fn download_fabric_api(
    mc_version: &str,
    mods_dir: &Path,
) -> Result<String> {
    // Query Modrinth API for Fabric API versions
    let url = "https://api.modrinth.com/v2/project/fabric-api/version";
    let resp = client().get(url)
        .query(&[("loaders", "[\"fabric\"]")])
        .query(&[("game_versions", &format!("[\"{}\"]", mc_version))])
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(NimbusError::Http {
            status: resp.status().as_u16(),
            url: url.to_owned(),
            retriable: false,
        });
    }

    #[derive(Deserialize)]
    struct ModrinthFile {
        url: String,
        filename: String,
    }

    #[derive(Deserialize)]
    struct ModrinthVersion {
        files: Vec<ModrinthFile>,
    }

    let versions: Vec<ModrinthVersion> = resp.json().await?;

    let primary_file = versions
        .first()
        .and_then(|v| v.files.first())
        .ok_or_else(|| {
            NimbusError::Invalid("Fabric API не найден для этой версии Minecraft".into())
        })?;

    let dest = mods_dir.join(&primary_file.filename);
    tokio::fs::create_dir_all(mods_dir).await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ProgressEvent>();
    tokio::spawn(async move { while rx.recv().await.is_some() {} });

    download_one(
        DownloadTask {
            url: primary_file.url.clone(),
            dest: dest.clone(),
            hash: None,
            size: None,
        },
        tx,
    )
    .await?;

    Ok(primary_file.filename.clone())
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Returns the list of available loader versions for the given Minecraft version.
pub async fn list_loader_versions(
    loader: &ModLoader,
    mc_version: &str,
) -> Result<Vec<LoaderVersionInfo>> {
    match loader {
        ModLoader::Fabric => {
            list_meta_versions("https://meta.fabricmc.net/v2", mc_version).await
        }
        ModLoader::Quilt => {
            list_meta_versions("https://meta.quiltmc.org/v3", mc_version).await
        }
        ModLoader::Forge => list_forge_versions(mc_version).await,
        ModLoader::NeoForge => list_neoforge_versions(mc_version).await,
    }
}

/// Downloads and caches the loader profile JSON. Returns the profile ID
/// (e.g. `fabric-loader-0.16.0-1.21`).
pub async fn download_loader_profile(
    loader: &ModLoader,
    mc_version: &str,
    loader_version: &str,
    progress: Option<crate::download::ProgressSender>,
) -> Result<String> {
    match loader {
        ModLoader::Fabric => {
            download_meta_profile(
                "https://meta.fabricmc.net/v2",
                mc_version,
                loader_version,
                loader,
                progress,
            )
            .await?;
        }
        ModLoader::Quilt => {
            download_meta_profile(
                "https://meta.quiltmc.org/v3",
                mc_version,
                loader_version,
                loader,
                progress,
            )
            .await?;
        }
        ModLoader::Forge => {
            download_forge_profile(mc_version, loader_version, progress).await?;
        }
        ModLoader::NeoForge => {
            download_neoforge_profile(mc_version, loader_version, progress).await?;
        }
    }
    Ok(profile_id(loader, loader_version, mc_version))
}

/// Downloads loader-specific libraries (for cases where the loader profile
/// adds libraries that need downloading). Returns the list of downloaded lib
/// paths relative to the libraries root.
pub async fn download_loader_libraries(
    loader: &ModLoader,
    _mc_version: &str,
    _loader_version: &str,
    _libraries_root: &Path,
    _loader_profile: &version::VersionMeta,
) -> Result<Vec<String>> {
    match loader {
        ModLoader::Fabric | ModLoader::Quilt => {
            // Fabric/Quilt libraries are already covered by the profile's
            // library list, which gets merged via inheritsFrom in the
            // existing `install_version` pipeline.
            Ok(Vec::new())
        }
        ModLoader::Forge | ModLoader::NeoForge => {
            // Forge and NeoForge may have additional artifacts that need
            // downloading. The profile JSON's library list is merged
            // automatically during `fetch_any_version` resolution.
            // Any custom URL libraries are handled by modifying
            // `resolve_libraries` to use the URL from the library entry.
            // The actual downloading happens in `install_version`'s
            // library download step which processes all resolved libs.
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_id_format() {
        assert_eq!(
            profile_id(&ModLoader::Fabric, "0.16.0", "1.21"),
            "fabric-loader-0.16.0-1.21"
        );
        assert_eq!(
            profile_id(&ModLoader::Forge, "47.1.30", "1.20.1"),
            "forge-loader-47.1.30-1.20.1"
        );
    }

    #[test]
    fn forge_extracts_mc_version() {
        assert_eq!(
            forge_mc_version("1.20.1-47.1.30"),
            Some("1.20.1".to_owned())
        );
        assert_eq!(forge_mc_version("1.21-47.1.0"), Some("1.21".to_owned()));
    }

    #[test]
    fn neoforge_to_mc_mapping() {
        // NeoForge 21.0.x → MC 1.21 (no patch component)
        assert_eq!(
            neoforge_to_mc_prefix("21.0.0-beta"),
            Some("1.21".to_owned())
        );
        // NeoForge 20.4.x → MC 1.20.4
        assert_eq!(
            neoforge_to_mc_prefix("20.4.1"),
            Some("1.20.4".to_owned())
        );
        // Edge: NeoForge 20.2.0 → MC 1.20.2 (trailing .0 stripped)
        assert_eq!(
            neoforge_to_mc_prefix("20.2.0"),
            Some("1.20.2".to_owned())
        );
    }

    #[test]
    fn parse_maven_xml_simple() {
        let xml = r#"<?xml version="1.0"?>
<metadata>
  <groupId>net.neoforged</groupId>
  <artifactId>neoforge</artifactId>
  <versioning>
    <versions>
      <version>21.0.0-beta</version>
      <version>20.4.0</version>
    </versions>
  </versioning>
</metadata>"#;
        let versions = parse_maven_metadata_xml(xml);
        assert_eq!(versions, vec!["21.0.0-beta".to_owned(), "20.4.0".to_owned()]);
    }

    #[test]
    fn loader_display_names() {
        assert_eq!(ModLoader::Fabric.display_name(), "Fabric");
        assert_eq!(ModLoader::Quilt.display_name(), "Quilt");
        assert_eq!(ModLoader::Forge.display_name(), "Forge");
        assert_eq!(ModLoader::NeoForge.display_name(), "NeoForge");
    }

    #[test]
    fn loader_roundtrip_str() {
        for loader in ModLoader::all() {
            assert_eq!(
                ModLoader::from_str(loader.as_str()),
                Some(loader.clone())
            );
        }
        assert_eq!(ModLoader::from_str("unknown"), None);
    }
}
