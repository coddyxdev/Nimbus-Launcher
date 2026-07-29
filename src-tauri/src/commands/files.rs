//! Folder shortcuts, crash reports, file export and shared-cache cleanup.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{NimbusError, Result};
use crate::instance;
use crate::paths;

use super::shared::reveal_dir;

/// Opens the instance's game directory (`.minecraft`) in Explorer.
#[tauri::command]
pub async fn open_game_dir(instance_id: String) -> Result<()> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    reveal_dir(&inst.game_dir(&instances_dir)).await
}

/// Opens the instance's mods directory in Explorer.
#[tauri::command]
pub async fn open_mods_dir(instance_id: String) -> Result<()> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    reveal_dir(&inst.mods_dir(&instances_dir)).await
}

/// Opens the instance's screenshots directory in Explorer.
#[tauri::command]
pub async fn open_screenshots_dir(instance_id: String) -> Result<()> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    reveal_dir(&inst.game_dir(&instances_dir).join("screenshots")).await
}

/// Opens the instance's crash-reports directory in Explorer.
#[tauri::command]
pub async fn open_crash_reports_dir(instance_id: String) -> Result<()> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    reveal_dir(&inst.game_dir(&instances_dir).join("crash-reports")).await
}

/// Opens the launcher's own log directory for an instance.
#[tauri::command]
pub async fn open_logs_dir(instance_id: String) -> Result<()> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    reveal_dir(&inst.logs_dir(&instances_dir)).await
}

/// Metadata for a crash report file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReportInfo {
    file_name: String,
    size_bytes: u64,
    last_modified: u64,
}

#[tauri::command]
pub fn list_crash_reports(instance_id: String) -> Result<Vec<CrashReportInfo>> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let dir = inst.game_dir(&instances_dir).join("crash-reports");

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut reports: Vec<CrashReportInfo> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "txt" || e == "log").unwrap_or(false) {
            if let Ok(metadata) = std::fs::metadata(&path) {
                reports.push(CrashReportInfo {
                    file_name: entry.file_name().to_string_lossy().to_string(),
                    size_bytes: metadata.len(),
                    last_modified: metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                });
            }
        }
    }
    reports.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
    Ok(reports)
}

/// Reads one crash report so the UI can show it without opening Explorer.
///
/// The file name is validated, so this cannot be used to read arbitrary paths.
#[tauri::command]
pub async fn read_crash_report(instance_id: String, file_name: String) -> Result<String> {
    use super::shared::validate_file_name;

    validate_file_name(&file_name)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let path = inst
        .game_dir(&instances_dir)
        .join("crash-reports")
        .join(&file_name);

    if !path.is_file() {
        return Err(NimbusError::Invalid("Краш-репорт не найден".to_owned()));
    }

    // Crash reports are a few dozen KB; a cap keeps a pathological file from
    // freezing the WebView.
    const MAX_BYTES: u64 = 4 * 1024 * 1024;
    let size = tokio::fs::metadata(&path).await?.len();
    if size > MAX_BYTES {
        return Err(NimbusError::Invalid(
            "Краш-репорт слишком большой — откройте его в папке".to_owned(),
        ));
    }

    let bytes = tokio::fs::read(&path).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Extensions the launcher is willing to write through [`save_text_file`].
const EXPORTABLE_EXTENSIONS: [&str; 5] = ["log", "txt", "json", "csv", "md"];

/// Writes UTF-8 text to an absolute path chosen by the user in a save dialog.
/// Used by the console "export log" action.
///
/// The path comes from the frontend, so it is treated as untrusted: only plain
/// text extensions are allowed and system locations are refused. That way the
/// command cannot be turned into a general "write anywhere" primitive.
#[tauri::command]
pub async fn save_text_file(path: String, contents: String) -> Result<()> {
    let target = Path::new(&path);
    if !target.is_absolute() {
        return Err(NimbusError::Invalid(
            "ожидался абсолютный путь для сохранения".to_owned(),
        ));
    }
    // UNC and verbatim prefixes bypass the normalisation below.
    if path.starts_with(r"\\") {
        return Err(NimbusError::Invalid(
            "сетевые пути не поддерживаются".to_owned(),
        ));
    }
    if target.components().any(|c| c.as_os_str() == "..") {
        return Err(NimbusError::Invalid("недопустимый путь".to_owned()));
    }

    let allowed_ext = target
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .map(|e| EXPORTABLE_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or(false);
    if !allowed_ext {
        return Err(NimbusError::Invalid(format!(
            "сохранять можно только текстовые файлы ({})",
            EXPORTABLE_EXTENSIONS.join(", ")
        )));
    }

    // Never write into Windows or the installed program directories.
    let forbidden: Vec<std::path::PathBuf> = ["WINDIR", "ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(|key| std::env::var_os(key))
        .map(std::path::PathBuf::from)
        .collect();
    let lower = path.to_ascii_lowercase();
    if forbidden
        .iter()
        .any(|dir| lower.starts_with(&dir.to_string_lossy().to_ascii_lowercase()))
    {
        return Err(NimbusError::Invalid(
            "запись в системные папки запрещена".to_owned(),
        ));
    }

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(target, contents.as_bytes()).await?;
    Ok(())
}

/// Result of a shared-cache cleanup pass.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    removed_files: u64,
    freed_bytes: u64,
}

/// Removes cached installer jars, their `.processed-*` markers and directories
/// whose names contain `:` (left behind by older Forge installer runs on
/// Windows, where such names are unusable).
#[tauri::command]
pub async fn cleanup_shared() -> Result<CleanupReport> {
    let shared_dir = paths::shared_dir()?;

    tokio::task::spawn_blocking(move || {
        let mut removed_files = 0u64;
        let mut freed_bytes = 0u64;

        let installers = shared_dir.join("installers");
        if let Ok(entries) = std::fs::read_dir(&installers) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let is_installer = name.ends_with(".jar");
                let is_marker = name.starts_with(".processed-");
                if !(is_installer || is_marker) {
                    continue;
                }
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if std::fs::remove_file(&path).is_ok() {
                    removed_files += 1;
                    freed_bytes += size;
                }
            }
        }

        // Junk directories with a colon in the name, e.g.
        // `libraries/net/minecraft/client/1.21:mappings@tsrg`.
        fn sweep_colons(dir: &Path, removed: &mut u64, freed: &mut u64, depth: u32) {
            if depth == 0 {
                return;
            }
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else { continue };
                if !file_type.is_dir() {
                    continue;
                }
                let path = entry.path();
                if entry.file_name().to_string_lossy().contains(':') {
                    let size = instance::dir_size(&path);
                    if std::fs::remove_dir_all(&path).is_ok() {
                        *removed += 1;
                        *freed += size;
                    }
                } else {
                    sweep_colons(&path, removed, freed, depth - 1);
                }
            }
        }
        sweep_colons(&shared_dir.join("libraries"), &mut removed_files, &mut freed_bytes, 12);

        CleanupReport {
            removed_files,
            freed_bytes,
        }
    })
    .await
    .map_err(|e| NimbusError::Invalid(format!("cleanup task failed: {e}")))
}
