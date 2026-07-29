//! Instance listing, deletion, duplication, renaming and per-instance settings.

use std::path::Path;

use tauri::AppHandle;

use crate::error::{NimbusError, Result};
use crate::instance::{self, Instance, InstanceSettings};
use crate::loader::{self, ModLoader};
use crate::paths;
use crate::version::{self, VersionSummary};

use super::shared::{ensure_not_running, validate_instance_name};

#[tauri::command]
pub fn list_instances() -> Result<Vec<Instance>> {
    let instances_dir = paths::instances_dir()?;
    instance::load_all(&instances_dir)
}

#[tauri::command]
pub async fn list_versions(include_snapshots: bool) -> Result<Vec<VersionSummary>> {
    version::list_versions(include_snapshots).await
}

/// Returns available loader versions for the given Minecraft version and loader type.
#[tauri::command]
pub async fn list_loader_versions(
    loader: String,
    mc_version: String,
) -> Result<Vec<loader::LoaderVersionInfo>> {
    let loader = ModLoader::from_str(&loader)
        .ok_or_else(|| NimbusError::Invalid(format!("unknown loader: {loader}")))?;
    loader::list_loader_versions(&loader, &mc_version).await
}

#[tauri::command]
pub async fn delete_instance(instance_id: String, app: AppHandle) -> Result<()> {
    ensure_not_running(&app, &instance_id)?;
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let dir = inst.dir(&instances_dir);
    tokio::fs::remove_dir_all(&dir).await?;
    Ok(())
}

/// Recursively copies a directory tree.
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Duplicates an instance with a new name.
#[tauri::command]
pub async fn duplicate_instance(
    instance_id: String,
    new_name: String,
    app: AppHandle,
) -> Result<Instance> {
    ensure_not_running(&app, &instance_id)?;
    let new_name = validate_instance_name(&new_name)?;
    let instances_dir = paths::instances_dir()?;
    let source = instance::load(&instances_dir, &instance_id)?;
    let new_id = instance::new_id_for_copy(&source.version_id);

    let source_dir = source.dir(&instances_dir);
    let dest_dir = instances_dir.join(&new_id);

    // Instances weigh gigabytes; copying them on the async runtime blocks
    // every other command until the copy finishes.
    {
        let src = source_dir.clone();
        let dst = dest_dir.clone();
        tokio::task::spawn_blocking(move || copy_dir(&src, &dst))
            .await
            .map_err(|e| NimbusError::Invalid(format!("copy task failed: {e}")))??;
    }

    let mut new_inst = Instance {
        id: new_id,
        name: new_name,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        ..source
    };
    new_inst.last_played = None;
    // A copy has not been played yet; carrying the source's counter over would
    // report time the user never spent in this instance.
    new_inst.total_playtime_secs = None;

    instance::save(&instances_dir, &new_inst)?;
    Ok(new_inst)
}

/// Renames an instance.
#[tauri::command]
pub async fn rename_instance(instance_id: String, new_name: String) -> Result<Instance> {
    let new_name = validate_instance_name(&new_name)?;
    let instances_dir = paths::instances_dir()?;
    let mut inst = instance::load(&instances_dir, &instance_id)?;
    inst.name = new_name;
    instance::save(&instances_dir, &inst)?;
    Ok(inst)
}

/// Total size of the instance directory in bytes. Walking a large instance can
/// take a moment, so it runs on the blocking pool.
#[tauri::command]
pub async fn instance_size(instance_id: String) -> Result<u64> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let dir = inst.dir(&instances_dir);
    tokio::task::spawn_blocking(move || instance::dir_size(&dir))
        .await
        .map_err(|e| NimbusError::Invalid(format!("size task failed: {e}")))
}

/// Replaces per-instance launch overrides. Passing `null` clears them and the
/// instance falls back to the global settings.
#[tauri::command]
pub async fn set_instance_settings(
    instance_id: String,
    settings: Option<InstanceSettings>,
) -> Result<Instance> {
    let instances_dir = paths::instances_dir()?;
    let settings = settings.map(|mut s| {
        s.memory_mib = s.memory_mib.map(|m| m.clamp(512, 65_536));
        s
    });
    instance::set_settings(&instances_dir, &instance_id, settings)
}
