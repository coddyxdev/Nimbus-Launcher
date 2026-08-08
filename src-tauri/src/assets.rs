//! Minecraft asset downloading and layout.
//!
//! Three asset modes are supported:
//! - Default: objects stored at `assets/objects/{h[0..2]}/{h}`.
//! - `map_to_resources: true` (very old versions): copy to `<instance>/resources/{name}`.
//! - `virtual: true` (1.7.x "legacy"): copy to `assets/virtual/legacy/{name}`.
//!
//! All three are required for correct audio and font loading in 1.8.9 and older.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;

use crate::download::{download_many, hash_file, ExpectedHash, DownloadTask, ProgressEvent};
use crate::error::{NimbusError, Result};
use crate::version::{AssetIndex, VersionMeta};

const ASSET_BASE_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct AssetIndexData {
    objects: std::collections::HashMap<String, AssetObject>,
    #[serde(default)]
    map_to_resources: bool,
    #[serde(default)]
    r#virtual: bool,
}

fn object_path(assets_root: &Path, hash: &str) -> PathBuf {
    assets_root
        .join("objects")
        .join(&hash[..2])
        .join(hash)
}

async fn fetch_asset_index(
    idx: &AssetIndex,
    assets_root: &Path,
) -> Result<AssetIndexData> {
    let cache = assets_root
        .join("indexes")
        .join(format!("{}.json", idx.id));
    let cache_parent = cache.parent().ok_or_else(|| {
        NimbusError::Invalid("asset index cache path has no parent".to_owned())
    })?;
    tokio::fs::create_dir_all(cache_parent).await?;

    // Use cached copy when hash matches.
    if cache.exists() {
        let actual = hash_file(&cache, "sha1").await?;
        if actual == idx.sha1 {
            let text = tokio::fs::read_to_string(&cache).await?;
            return Ok(serde_json::from_str(&text)?);
        }
    }

    let resp = crate::download::client().get(&idx.url).send().await?;
    let body = resp.text().await?;
    crate::config::write_atomic(&cache, body.as_bytes())?;
    Ok(serde_json::from_str(&body)?)
}

/// Downloads the asset index and all referenced objects. Handles
/// `map_to_resources` and `virtual` layouts by copying after download.
pub async fn install_assets(
    meta: &VersionMeta,
    instance_dir: &Path,
    assets_root: &Path,
    progress: UnboundedSender<ProgressEvent>,
) -> Result<()> {
    let Some(idx) = &meta.asset_index else {
        // Very old versions (alpha/beta) may have no assetIndex at all.
        return Ok(());
    };

    let data = fetch_asset_index(idx, assets_root).await?;

    let mut tasks: Vec<DownloadTask> = Vec::with_capacity(data.objects.len());
    for obj in data.objects.values() {
        let dest = object_path(assets_root, &obj.hash);
        tasks.push(DownloadTask {
            url: format!(
                "{}/{}/{}",
                ASSET_BASE_URL,
                &obj.hash[..2],
                obj.hash
            ),
            dest,
            hash: Some(ExpectedHash::Sha1(obj.hash.clone())),
            size: Some(obj.size),
        });
    }

    download_many(tasks, progress).await?;

    // Post-download: apply map_to_resources / virtual layouts by copying files.
    if data.map_to_resources || data.r#virtual {
        let target_root = if data.map_to_resources {
            instance_dir.join("resources")
        } else {
            assets_root.join("virtual").join("legacy")
        };
        tokio::fs::create_dir_all(&target_root).await?;

        for (name, obj) in &data.objects {
            let src = object_path(assets_root, &obj.hash);
            let dest = match safe_join(&target_root, name) {
                Some(p) => p,
                None => {
                    return Err(NimbusError::Invalid(format!(
                        "path traversal in asset name: {name}"
                    )));
                }
            };
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            if !dest.exists() {
                tokio::fs::copy(&src, &dest).await?;
            }
        }
    }

    Ok(())
}

/// Returns the path to the asset index directory (for the `assets_root`
/// placeholder in version arguments).
pub fn assets_root_path(shared_dir: &Path) -> PathBuf {
    shared_dir.join("assets")
}

// Path-traversal-safe join is shared across the codebase; see paths::safe_join.
use crate::paths::safe_join;
