//! Modrinth-backed commands: search, version listing, one-click install.

use crate::error::Result;
use crate::instance;
use crate::modrinth::{self, ModrinthProject, ModrinthSearchPage, ModrinthVersion};
use crate::paths;

use super::mods::ModInfo;

/// Loader/MC version defaults come from the instance itself, so the UI never
/// has to pass them and results are always compatible with what is installed.
async fn instance_context(instance_id: &str) -> Result<(Option<String>, Option<String>)> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, instance_id)?;
    let mc = inst
        .minecraft_version
        .clone()
        .or_else(|| Some(inst.version_id.clone()));
    Ok((inst.loader.clone(), mc))
}

/// Searches Modrinth mods compatible with the given instance.
#[tauri::command]
pub async fn modrinth_search(
    instance_id: String,
    query: String,
    limit: Option<u32>,
    offset: Option<u32>,
    sort: Option<String>,
) -> Result<ModrinthSearchPage> {
    let (loader, mc) = instance_context(&instance_id).await?;
    modrinth::search(
        query.trim(),
        loader.as_deref(),
        mc.as_deref(),
        limit.unwrap_or(30),
        offset.unwrap_or(0),
        sort.as_deref(),
    )
    .await
}

/// Lists the compatible versions of a Modrinth project.
#[tauri::command]
pub async fn modrinth_versions(
    instance_id: String,
    project_id: String,
) -> Result<Vec<ModrinthVersion>> {
    let (loader, mc) = instance_context(&instance_id).await?;
    modrinth::versions(&project_id, loader.as_deref(), mc.as_deref()).await
}

/// Loads the full project page (long description, gallery, links) shown in
/// the in-app mod details view.
#[tauri::command]
pub async fn modrinth_project(project_id: String) -> Result<ModrinthProject> {
    modrinth::project(&project_id).await
}

/// Lists every published version of a project without narrowing by instance.
/// The details view uses this for modpacks, which have no instance context.
#[tauri::command]
pub async fn modrinth_project_versions(project_id: String) -> Result<Vec<ModrinthVersion>> {
    modrinth::versions(&project_id, None, None).await
}

/// Installs a mod into the instance. Without `version_id` the newest
/// compatible release is picked automatically.
#[tauri::command]
pub async fn modrinth_install(
    instance_id: String,
    project_id: String,
    version_id: Option<String>,
) -> Result<ModInfo> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let mods_dir = inst.mods_dir(&instances_dir);

    let mc = inst
        .minecraft_version
        .clone()
        .or_else(|| Some(inst.version_id.clone()));

    let version = match version_id {
        Some(vid) => {
            let list =
                modrinth::versions(&project_id, inst.loader.as_deref(), mc.as_deref()).await?;
            match list.into_iter().find(|v| v.id == vid) {
                Some(v) => v,
                // The requested version may be filtered out by the facets; fall
                // back to the newest compatible one instead of failing.
                None => {
                    modrinth::best_version(&project_id, inst.loader.as_deref(), mc.as_deref())
                        .await?
                }
            }
        }
        None => modrinth::best_version(&project_id, inst.loader.as_deref(), mc.as_deref()).await?,
    };

    let file_name = modrinth::install_version(&mods_dir, &version).await?;
    let metadata = std::fs::metadata(mods_dir.join(&file_name))?;

    Ok(ModInfo {
        file_name,
        size_bytes: metadata.len(),
        last_modified: metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0),
        enabled: true,
    })
}
