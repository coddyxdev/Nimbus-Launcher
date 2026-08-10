//! Mod file management inside an instance's `mods` directory.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{NimbusError, Result};
use crate::instance;
use crate::paths;

use super::shared::validate_file_name;

/// Disabled mods keep their bytes but get an extra suffix so no loader picks
/// them up. This is the same convention Prism/MultiMC use.
const DISABLED_SUFFIX: &str = ".disabled";

/// Information about a single mod file in the instance's mods directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    /// Always the enabled form (`name.jar`), even when the file on disk is
    /// `name.jar.disabled`, so the UI has one stable identity per mod.
    pub file_name: String,
    pub size_bytes: u64,
    pub last_modified: u64,
    pub enabled: bool,
}

fn modified_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Splits an on-disk name into (canonical jar name, enabled).
fn classify(name: &str) -> Option<(String, bool)> {
    if let Some(stripped) = name.strip_suffix(DISABLED_SUFFIX) {
        if stripped.ends_with(".jar") {
            return Some((stripped.to_owned(), false));
        }
        return None;
    }
    if name.ends_with(".jar") {
        return Some((name.to_owned(), true));
    }
    None
}

/// Lists all mod files (enabled and disabled) in the instance's mods directory.
#[tauri::command]
pub fn list_mods(instance_id: String) -> Result<Vec<ModInfo>> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let mods_dir = inst.mods_dir(&instances_dir);

    if !mods_dir.exists() {
        return Ok(Vec::new());
    }

    let mut mods: Vec<ModInfo> = Vec::new();
    for entry in std::fs::read_dir(&mods_dir)? {
        let entry = entry?;
        let raw_name = entry.file_name().to_string_lossy().to_string();
        let Some((file_name, enabled)) = classify(&raw_name) else {
            continue;
        };
        let metadata = entry.metadata()?;
        mods.push(ModInfo {
            file_name,
            size_bytes: metadata.len(),
            last_modified: modified_secs(&metadata),
            enabled,
        });
    }
    // Sort alphabetically, case-insensitively, so the list matches what the
    // user sees in Explorer.
    mods.sort_by_key(|m| m.file_name.to_lowercase());
    Ok(mods)
}

/// Copies a .jar file into the instance's mods directory.
#[tauri::command]
pub async fn add_mod(instance_id: String, source_path: String) -> Result<ModInfo> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let mods_dir = inst.mods_dir(&instances_dir);
    tokio::fs::create_dir_all(&mods_dir).await?;

    let src = Path::new(&source_path);
    let file_name = src
        .file_name()
        .ok_or_else(|| NimbusError::Invalid("неверный путь к файлу".into()))?
        .to_string_lossy()
        .to_string();

    if !file_name.ends_with(".jar") {
        return Err(NimbusError::Invalid("Мод должен быть .jar файлом".into()));
    }
    validate_file_name(&file_name)?;

    let dest = mods_dir.join(&file_name);
    let disabled_dest = mods_dir.join(format!("{file_name}{DISABLED_SUFFIX}"));
    if dest.exists() || disabled_dest.exists() {
        return Err(NimbusError::Invalid(format!(
            "Мод '{file_name}' уже существует"
        )));
    }

    tokio::fs::copy(src, &dest).await?;

    let metadata = std::fs::metadata(&dest)?;
    Ok(ModInfo {
        file_name,
        size_bytes: metadata.len(),
        last_modified: modified_secs(&metadata),
        enabled: true,
    })
}

/// Removes a mod, whether it is currently enabled or disabled.
#[tauri::command]
pub async fn remove_mod(instance_id: String, file_name: String) -> Result<()> {
    validate_file_name(&file_name)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let mods_dir = inst.mods_dir(&instances_dir);

    let enabled_path = mods_dir.join(&file_name);
    let disabled_path = mods_dir.join(format!("{file_name}{DISABLED_SUFFIX}"));
    let path = if enabled_path.exists() {
        enabled_path
    } else if disabled_path.exists() {
        disabled_path
    } else {
        return Err(NimbusError::Invalid(format!("Мод '{file_name}' не найден")));
    };

    tokio::fs::remove_file(&path).await?;
    Ok(())
}

/// Enables or disables a mod by renaming it, without deleting anything.
#[tauri::command]
pub async fn set_mod_enabled(
    instance_id: String,
    file_name: String,
    enabled: bool,
) -> Result<ModInfo> {
    validate_file_name(&file_name)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let mods_dir = inst.mods_dir(&instances_dir);

    let enabled_path = mods_dir.join(&file_name);
    let disabled_path = mods_dir.join(format!("{file_name}{DISABLED_SUFFIX}"));

    let (from, to) = if enabled {
        (disabled_path.clone(), enabled_path.clone())
    } else {
        (enabled_path.clone(), disabled_path.clone())
    };

    if !from.exists() {
        // Already in the requested state: report the current file instead of
        // failing, so double clicks in the UI are harmless.
        if to.exists() {
            let metadata = std::fs::metadata(&to)?;
            return Ok(ModInfo {
                file_name,
                size_bytes: metadata.len(),
                last_modified: modified_secs(&metadata),
                enabled,
            });
        }
        return Err(NimbusError::Invalid(format!("Мод '{file_name}' не найден")));
    }

    tokio::fs::rename(&from, &to).await?;
    let metadata = std::fs::metadata(&to)?;
    Ok(ModInfo {
        file_name,
        size_bytes: metadata.len(),
        last_modified: modified_secs(&metadata),
        enabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_recognises_both_states() {
        assert_eq!(
            classify("sodium.jar"),
            Some(("sodium.jar".to_owned(), true))
        );
        assert_eq!(
            classify("sodium.jar.disabled"),
            Some(("sodium.jar".to_owned(), false))
        );
        assert_eq!(classify("readme.txt"), None);
        assert_eq!(classify("notes.txt.disabled"), None);
    }
}
