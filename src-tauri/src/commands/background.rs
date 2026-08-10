//! Custom launcher background: a still image, an animated GIF or a short
//! video that plays behind the whole UI.
//!
//! The picked file is copied into `<root>/backgrounds` instead of being
//! referenced in place. A background that lives inside the launcher profile
//! cannot break because the user moved, renamed or deleted the original, and
//! it keeps the asset-protocol scope down to a single directory we own.
//!
//! Bytes never cross the IPC bridge: the frontend renders the copy through
//! `convertFileSrc`, exactly like the screenshot gallery does.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::config;
use crate::error::{NimbusError, Result};
use crate::paths;

/// Still and animated formats every WebView2 build can decode.
const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

/// Video containers WebView2 plays without extra codecs.
const VIDEO_EXTENSIONS: [&str; 2] = ["mp4", "webm"];

/// Size ceilings. A background is decoded for the entire lifetime of the
/// window, so an unbounded file would sit in memory the whole session and, for
/// video, decode every frame forever. These limits keep a wallpaper a
/// wallpaper rather than a media player.
const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_VIDEO_BYTES: u64 = 60 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundInfo {
    pub file_name: String,
    /// Absolute path; the frontend feeds it to `convertFileSrc`.
    pub path: String,
    /// `"image"` or `"video"` — decides whether the UI mounts <img> or <video>.
    pub kind: String,
    pub size_bytes: u64,
}

fn kind_for(extension: &str) -> Option<&'static str> {
    if IMAGE_EXTENSIONS.contains(&extension) {
        Some("image")
    } else if VIDEO_EXTENSIONS.contains(&extension) {
        Some("video")
    } else {
        None
    }
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn human_mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

/// Deletes every file in the backgrounds folder except `keep`.
///
/// Each import gets a fresh timestamped name so the WebView cannot serve the
/// previous picture from its cache; without this sweep those superseded copies
/// would pile up in the profile forever.
fn prune_others(dir: &Path, keep: Option<&str>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if Some(name.as_str()) == keep {
            continue;
        }
        if let Err(err) = std::fs::remove_file(&path) {
            crate::nlog!("background: could not remove old file {name} ({err})");
        }
    }
}

/// Copies the picked file into the profile and records it in the config.
#[tauri::command]
pub async fn set_background(source_path: String) -> Result<BackgroundInfo> {
    let source = PathBuf::from(&source_path);
    let extension = extension_of(&source);
    let kind = kind_for(&extension).ok_or_else(|| {
        NimbusError::Invalid(
            "Подойдёт PNG, JPG, GIF, WEBP, MP4 или WEBM — другие форматы лаунчер показать не сможет"
                .to_owned(),
        )
    })?;

    let meta = tokio::fs::metadata(&source)
        .await
        .map_err(|_| NimbusError::Invalid("Файл не найден".to_owned()))?;
    if !meta.is_file() {
        return Err(NimbusError::Invalid(
            "Выбрать нужно файл, а не папку".to_owned(),
        ));
    }

    let limit = if kind == "video" {
        MAX_VIDEO_BYTES
    } else {
        MAX_IMAGE_BYTES
    };
    if meta.len() > limit {
        return Err(NimbusError::Invalid(format!(
            "Файл весит {} МБ, а максимум — {} МБ. Сожмите его или возьмите вариант полегче.",
            human_mib(meta.len()).max(1),
            human_mib(limit)
        )));
    }

    let dir = paths::backgrounds_dir()?;
    tokio::fs::create_dir_all(&dir).await?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let file_name = format!("background-{stamp}.{extension}");
    let dest = dir.join(&file_name);
    tokio::fs::copy(&source, &dest).await?;

    let sweep_dir = dir.clone();
    let sweep_keep = file_name.clone();
    let _ = tokio::task::spawn_blocking(move || prune_others(&sweep_dir, Some(&sweep_keep))).await;

    let stored_name = file_name.clone();
    let stored_kind = kind.to_owned();
    config::update(move |cfg| {
        cfg.background_file = Some(stored_name);
        cfg.background_kind = Some(stored_kind);
        Ok(())
    })?;

    Ok(BackgroundInfo {
        file_name,
        path: dest.to_string_lossy().into_owned(),
        kind: kind.to_owned(),
        size_bytes: meta.len(),
    })
}

/// The background currently in use, or `None` when the user has not set one.
///
/// A config entry whose file vanished (profile copied between machines, manual
/// cleanup) resolves to `None` and clears itself, so the UI never renders a
/// broken image.
#[tauri::command]
pub async fn get_background() -> Result<Option<BackgroundInfo>> {
    let cfg = config::load()?;
    let Some(file_name) = cfg.background_file.clone() else {
        return Ok(None);
    };

    let path = paths::backgrounds_dir()?.join(&file_name);
    let Ok(meta) = tokio::fs::metadata(&path).await else {
        config::update(|cfg| {
            cfg.background_file = None;
            cfg.background_kind = None;
            Ok(())
        })?;
        return Ok(None);
    };

    let extension = extension_of(&path);
    let kind = cfg
        .background_kind
        .clone()
        .or_else(|| kind_for(&extension).map(|k| k.to_owned()))
        .unwrap_or_else(|| "image".to_owned());

    Ok(Some(BackgroundInfo {
        file_name,
        path: path.to_string_lossy().into_owned(),
        kind,
        size_bytes: meta.len(),
    }))
}

/// Removes the background and its copy on disk.
#[tauri::command]
pub async fn clear_background() -> Result<()> {
    config::update(|cfg| {
        cfg.background_file = None;
        cfg.background_kind = None;
        Ok(())
    })?;

    let dir = paths::backgrounds_dir()?;
    let _ = tokio::task::spawn_blocking(move || prune_others(&dir, None)).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_formats_map_to_a_kind() {
        assert_eq!(kind_for("png"), Some("image"));
        assert_eq!(kind_for("gif"), Some("image"));
        assert_eq!(kind_for("mp4"), Some("video"));
        assert_eq!(kind_for("webm"), Some("video"));
    }

    #[test]
    fn unsupported_formats_are_rejected() {
        assert_eq!(kind_for("exe"), None);
        assert_eq!(kind_for("mkv"), None);
        assert_eq!(kind_for(""), None);
    }

    #[test]
    fn extension_is_lowercased() {
        assert_eq!(extension_of(Path::new("C:/pics/Wall.PNG")), "png");
        assert_eq!(extension_of(Path::new("C:/pics/clip.MP4")), "mp4");
        assert_eq!(extension_of(Path::new("C:/pics/noext")), "");
    }

    #[test]
    fn prune_keeps_only_the_current_file() {
        let dir = std::env::temp_dir().join(format!(
            "nimbus-bg-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("old.png"), b"old").expect("write old");
        std::fs::write(dir.join("new.png"), b"new").expect("write new");

        prune_others(&dir, Some("new.png"));

        assert!(!dir.join("old.png").exists());
        assert!(dir.join("new.png").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
