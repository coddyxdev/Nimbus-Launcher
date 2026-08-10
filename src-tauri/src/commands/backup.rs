//! Export/import an instance as a portable `.zip` backup.
//!
//! The archive contains `instance.json` (name, version, loader) at its root
//! plus the full `game/` directory (mods, saves, configs, resource packs...).
//! `natives/` and `logs/` are deliberately excluded: natives are re-extracted
//! on next launch and logs are launcher-local, so skipping them keeps the
//! archive smaller without losing anything the user cares about.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::AppHandle;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{NimbusError, Result};
use crate::instance::{self, Instance};
use crate::paths;

use super::shared::ensure_not_running;

fn zip_err(e: zip::result::ZipError) -> NimbusError {
    NimbusError::Zip(e.to_string())
}

/// Joins `rel` (a `/`-separated zip entry path) onto `base`, rejecting
/// traversal and absolute/drive-letter paths so a malicious archive cannot
/// write outside the instance's game directory.
fn safe_join(base: &Path, rel: &str) -> Result<PathBuf> {
    crate::paths::safe_join(base, rel)
        .ok_or_else(|| NimbusError::Invalid(format!("небезопасный путь в резервной копии: {rel}")))
}

/// Recursively adds every file under `dir` to the archive, using
/// `<prefix>/<path relative to dir>` as the entry name.
fn add_dir_recursive(
    zip: &mut ZipWriter<File>,
    base: &Path,
    dir: &Path,
    prefix: &str,
    options: SimpleFileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            add_dir_recursive(zip, base, &path, prefix, options)?;
            continue;
        }
        let rel = path
            .strip_prefix(base)
            .map_err(|_| NimbusError::Invalid("internal path error during export".to_owned()))?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let zip_path = format!("{prefix}/{rel_str}");
        zip.start_file(&zip_path, options).map_err(zip_err)?;
        let mut f = File::open(&path)?;
        std::io::copy(&mut f, zip)?;
    }
    Ok(())
}

fn export_zip_blocking(instance_dir: &Path, dest: &Path) -> Result<()> {
    let file = File::create(dest)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let json_path = instance_dir.join("instance.json");
    zip.start_file("instance.json", options).map_err(zip_err)?;
    let mut f = File::open(&json_path)?;
    std::io::copy(&mut f, &mut zip)?;

    let game_dir = instance_dir.join("game");
    if game_dir.is_dir() {
        add_dir_recursive(&mut zip, &game_dir, &game_dir, "game", options)?;
    }

    zip.finish().map_err(zip_err)?;
    Ok(())
}

/// Exports an instance's `game/` directory and metadata to a `.zip` file the
/// user can move to another PC and re-import. Refused while the instance is
/// running, since its files may be mid-write.
#[tauri::command]
pub async fn export_instance(instance_id: String, dest_path: String, app: AppHandle) -> Result<()> {
    ensure_not_running(&app, &instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let src_dir = inst.dir(&instances_dir);
    let dest = PathBuf::from(dest_path);
    tokio::task::spawn_blocking(move || export_zip_blocking(&src_dir, &dest))
        .await
        .map_err(|e| NimbusError::Invalid(format!("export task failed: {e}")))??;
    Ok(())
}

/// Subset of `instance.json` read back out of an imported archive. Optional
/// fields default to sensible values so archives from older launcher
/// versions still import.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ImportedMeta {
    #[serde(default = "default_name")]
    name: String,
    #[serde(default = "default_version")]
    version_id: String,
    #[serde(default)]
    loader: Option<String>,
    #[serde(default)]
    loader_version: Option<String>,
    #[serde(default)]
    minecraft_version: Option<String>,
}

fn default_name() -> String {
    "Импортированная сборка".to_owned()
}

fn default_version() -> String {
    "unknown".to_owned()
}

fn import_zip_blocking(
    zip_path: PathBuf,
    instances_dir: PathBuf,
    requested_name: Option<String>,
) -> Result<Instance> {
    let file = File::open(&zip_path)?;
    let mut archive = ZipArchive::new(file).map_err(zip_err)?;

    let meta: ImportedMeta = match archive.by_name("instance.json") {
        Ok(mut entry) => {
            let mut body = String::new();
            entry.read_to_string(&mut body)?;
            serde_json::from_str(&body).unwrap_or_default()
        }
        Err(_) => ImportedMeta::default(),
    };

    let new_id = instance::new_id_for_copy(&meta.version_id);
    let dest_dir = instances_dir.join(&new_id);
    let game_dir = dest_dir.join("game");
    std::fs::create_dir_all(&game_dir)?;
    std::fs::create_dir_all(dest_dir.join("natives"))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(zip_err)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let Some(rel) = name.strip_prefix("game/") else {
            continue;
        };
        let dest = safe_join(&game_dir, rel)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
    }

    let name = requested_name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(meta.name);

    let new_inst = Instance {
        id: new_id,
        name,
        version_id: meta.version_id,
        loader: meta.loader,
        loader_version: meta.loader_version,
        minecraft_version: meta.minecraft_version,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        last_played: None,
        installed: Some(true),
        settings: None,
        total_playtime_secs: None,
        modpack_source: None,
    };

    instance::save(&instances_dir, &new_inst)?;
    Ok(new_inst)
}

/// Imports a `.zip` backup previously produced by [`export_instance`] as a
/// new instance.
#[tauri::command]
pub async fn import_instance(path: String, instance_name: Option<String>) -> Result<Instance> {
    let zip_path = PathBuf::from(&path);
    if !zip_path.is_file() {
        return Err(NimbusError::Invalid(
            "Файл резервной копии не найден".to_owned(),
        ));
    }
    let instances_dir = paths::instances_dir()?;
    tokio::task::spawn_blocking(move || import_zip_blocking(zip_path, instances_dir, instance_name))
        .await
        .map_err(|e| NimbusError::Invalid(format!("import task failed: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_accepts_normal_relative_paths() {
        let base = Path::new("C:/instances/abc/game");
        assert!(safe_join(base, "mods/foo.jar").is_ok());
        assert!(safe_join(base, "config/sub/dir/file.toml").is_ok());
    }

    #[test]
    fn safe_join_rejects_traversal_and_absolute_paths() {
        let base = Path::new("C:/instances/abc/game");
        assert!(safe_join(base, "../escape.txt").is_err());
        assert!(safe_join(base, "mods/../../escape.txt").is_err());
        assert!(safe_join(base, "/abs/path").is_err());
        assert!(safe_join(base, "C:/abs/path").is_err());
        assert!(safe_join(base, "").is_err());
    }
}
