//! Screenshot gallery for an instance.
//!
//! Minecraft writes screenshots to `<game>/screenshots` and never shows them
//! again. Listing them in the launcher turns a dead folder into the feature
//! players actually want: look at the shot, copy it, or share it, without
//! digging through Explorer.
//!
//! Images are exposed as absolute paths; the frontend renders them through
//! Tauri's asset protocol (`convertFileSrc`), so no bytes cross the IPC
//! bridge.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{NimbusError, Result};
use crate::{instance, paths};

use super::shared::{validate_file_name, validate_instance_id};

/// Extensions Minecraft can produce in `screenshots`.
const IMAGE_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub file_name: String,
    /// Absolute path, for `convertFileSrc` and for "reveal in Explorer".
    pub path: String,
    pub size_bytes: u64,
    /// Unix seconds; the frontend formats it in the user's locale.
    pub modified: i64,
}

fn screenshots_dir(instance_id: &str) -> Result<PathBuf> {
    validate_instance_id(instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, instance_id)?;
    Ok(inst.game_dir(&instances_dir).join("screenshots"))
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Newest first, so the shot the player just took is the first thing they see.
#[tauri::command]
pub async fn list_screenshots(instance_id: String) -> Result<Vec<Screenshot>> {
    let dir = screenshots_dir(&instance_id)?;

    tokio::task::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            // A pack that has never been played has no screenshots folder;
            // that is an empty gallery, not an error.
            return Vec::new();
        };

        let mut shots: Vec<Screenshot> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if !path.is_file() || !is_image(&path) {
                    return None;
                }
                let meta = entry.metadata().ok()?;
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                Some(Screenshot {
                    file_name: path.file_name()?.to_string_lossy().into_owned(),
                    path: path.to_string_lossy().into_owned(),
                    size_bytes: meta.len(),
                    modified,
                })
            })
            .collect();

        shots.sort_by_key(|s| std::cmp::Reverse(s.modified));
        shots
    })
    .await
    .map_err(|err| NimbusError::Invalid(format!("screenshot scan failed: {err}")))
}

#[tauri::command]
pub async fn delete_screenshot(instance_id: String, file_name: String) -> Result<()> {
    validate_file_name(&file_name)?;
    let dir = screenshots_dir(&instance_id)?;
    let path = dir.join(&file_name);
    if !is_image(&path) {
        return Err(NimbusError::Invalid(
            "Удалять можно только изображения".to_owned(),
        ));
    }
    if path.exists() {
        tokio::fs::remove_file(&path).await?;
    }
    Ok(())
}

/// Copies a screenshot somewhere the user picked (Desktop, a Discord upload
/// folder, ...). The frontend supplies `dest_path` from a native save dialog,
/// which is what makes this the "share" action.
#[tauri::command]
pub async fn copy_screenshot(
    instance_id: String,
    file_name: String,
    dest_path: String,
) -> Result<String> {
    validate_file_name(&file_name)?;
    let source = screenshots_dir(&instance_id)?.join(&file_name);
    if !source.is_file() {
        return Err(NimbusError::Invalid(format!(
            "Скриншот {file_name} не найден"
        )));
    }

    let dest = PathBuf::from(&dest_path);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::copy(&source, &dest).await?;
    Ok(dest.to_string_lossy().into_owned())
}
