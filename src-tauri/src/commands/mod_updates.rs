//! Mod updates and dependency resolution.
//!
//! The launcher stores no metadata about installed mods. Instead every jar in
//! the instance's `mods` folder is hashed (SHA-1) and looked up on Modrinth,
//! which tells us the project and version each file belongs to. From there we
//! can compare against the newest compatible version and resolve the
//! dependencies a version declares.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::download::hash_file;
use crate::error::{NimbusError, Result};
use crate::instance;
use crate::modrinth;
use crate::paths;

const DISABLED_SUFFIX: &str = ".disabled";

/// An installed jar that Modrinth recognised.
struct Known {
    /// File name as it sits on disk, `.disabled` suffix included.
    disk_name: String,
    enabled: bool,
    version: modrinth::ModrinthVersion,
}

/// An available update for one installed mod.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModUpdate {
    /// Current file on disk (with `.disabled` if the mod is switched off).
    pub file_name: String,
    pub enabled: bool,
    pub project_id: String,
    pub title: String,
    pub icon_url: Option<String>,
    pub current_version: String,
    pub latest_version: String,
    pub latest_version_id: String,
    pub latest_file_name: String,
}

/// One dependency of a version, annotated with whether it is already present.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDependency {
    pub project_id: String,
    pub title: String,
    pub icon_url: Option<String>,
    /// `required` | `optional` | `incompatible` | `embedded`.
    pub dependency_type: String,
    pub installed: bool,
    /// Set when the dependency pins an exact version.
    pub version_id: Option<String>,
}

/// Result of installing a mod together with its dependencies.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallWithDepsReport {
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
}

fn instances_and_mods(instance_id: &str) -> Result<(PathBuf, PathBuf, Option<String>, Option<String>)> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, instance_id)?;
    let mods_dir = inst.mods_dir(&instances_dir);
    let mc = inst
        .minecraft_version
        .clone()
        .or_else(|| Some(inst.version_id.clone()));
    Ok((instances_dir, mods_dir, inst.loader.clone(), mc))
}

/// Hashes every jar in `mods_dir` and asks Modrinth what each one is.
async fn known_mods(mods_dir: &Path) -> Result<Vec<Known>> {
    if !mods_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files: Vec<(String, bool, PathBuf)> = Vec::new();
    let mut rd = tokio::fs::read_dir(mods_dir).await?;
    while let Some(entry) = rd.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let disk_name = entry.file_name().to_string_lossy().into_owned();
        let enabled = !disk_name.ends_with(DISABLED_SUFFIX);
        let base = disk_name.trim_end_matches(DISABLED_SUFFIX);
        if !base.ends_with(".jar") {
            continue;
        }
        files.push((disk_name, enabled, entry.path()));
    }

    let mut by_hash: HashMap<String, (String, bool)> = HashMap::new();
    for (disk_name, enabled, path) in files {
        // A jar that cannot be read is skipped rather than failing the scan.
        if let Ok(sha1) = hash_file(&path, "sha1").await {
            by_hash.insert(sha1, (disk_name, enabled));
        }
    }

    let hashes: Vec<String> = by_hash.keys().cloned().collect();
    let found = modrinth::versions_by_hashes(&hashes).await?;

    let mut out = Vec::new();
    for (hash, version) in found {
        if let Some((disk_name, enabled)) = by_hash.get(&hash) {
            out.push(Known {
                disk_name: disk_name.clone(),
                enabled: *enabled,
                version,
            });
        }
    }
    Ok(out)
}

/// Checks every recognised mod in the instance for a newer compatible version.
#[tauri::command]
pub async fn check_mod_updates(instance_id: String) -> Result<Vec<ModUpdate>> {
    let (_dirs, mods_dir, loader, mc) = instances_and_mods(&instance_id)?;
    let installed = known_mods(&mods_dir).await?;

    let mut updates = Vec::new();
    for item in installed {
        if item.version.project_id.is_empty() {
            continue;
        }
        let list = match modrinth::versions(
            &item.version.project_id,
            loader.as_deref(),
            mc.as_deref(),
        )
        .await
        {
            Ok(list) => list,
            // One unreachable project should not abort the whole check.
            Err(_) => continue,
        };

        // Modrinth returns newest first; prefer a release unless the project
        // only publishes pre-releases.
        let latest = list
            .iter()
            .find(|v| v.version_type == "release")
            .or_else(|| list.first());
        let Some(latest) = latest else { continue };
        if latest.id == item.version.id {
            continue;
        }
        let Some(file) = latest.primary_file() else {
            continue;
        };

        let project = modrinth::project(&item.version.project_id).await.ok();
        updates.push(ModUpdate {
            file_name: item.disk_name.clone(),
            enabled: item.enabled,
            project_id: item.version.project_id.clone(),
            title: project
                .as_ref()
                .map(|p| p.title.clone())
                .unwrap_or_else(|| item.disk_name.clone()),
            icon_url: project.as_ref().and_then(|p| p.icon_url.clone()),
            current_version: item.version.version_number.clone(),
            latest_version: latest.version_number.clone(),
            latest_version_id: latest.id.clone(),
            latest_file_name: file.filename.clone(),
        });
    }

    Ok(updates)
}

/// Replaces one installed jar with a newer version, keeping its on/off state.
#[tauri::command]
pub async fn apply_mod_update(
    instance_id: String,
    file_name: String,
    version_id: String,
) -> Result<String> {
    let (_dirs, mods_dir, _loader, _mc) = instances_and_mods(&instance_id)?;
    super::shared::validate_file_name(&file_name)?;

    let was_disabled = file_name.ends_with(DISABLED_SUFFIX);
    let old_path = mods_dir.join(&file_name);

    let version = modrinth::version_by_id(&version_id).await?;
    let installed = modrinth::install_version(&mods_dir, &version).await?;

    // Remove the old jar only after the new one landed successfully.
    if old_path.exists() && installed != file_name {
        let _ = tokio::fs::remove_file(&old_path).await;
    }

    if was_disabled {
        let from = mods_dir.join(&installed);
        let to = mods_dir.join(format!("{installed}{DISABLED_SUFFIX}"));
        tokio::fs::rename(&from, &to).await?;
        return Ok(format!("{installed}{DISABLED_SUFFIX}"));
    }

    Ok(installed)
}

/// Updates every mod that has a newer version. Failures are reported per mod
/// rather than aborting the batch.
#[tauri::command]
pub async fn apply_all_mod_updates(instance_id: String) -> Result<InstallWithDepsReport> {
    let updates = check_mod_updates(instance_id.clone()).await?;
    // A batch update is the most common way to break a working instance, so
    // capture the current jars first. Failing to snapshot must not block the
    // update itself (a full disk would otherwise make the launcher unusable).
    if !updates.is_empty() {
        if let Err(err) = super::restore::auto_snapshot(&instance_id, "Перед обновлением модов").await {
            eprintln!("[nimbus] restore: snapshot before mod update failed ({err})");
        }
    }

    let mut installed = Vec::new();
    let mut skipped = Vec::new();
    for upd in updates {
        match apply_mod_update(
            instance_id.clone(),
            upd.file_name.clone(),
            upd.latest_version_id.clone(),
        )
        .await
        {
            Ok(name) => installed.push(name),
            Err(_) => skipped.push(upd.title.clone()),
        }
    }

    Ok(InstallWithDepsReport { installed, skipped })
}

/// Resolves the version to install: the pinned one, or the newest compatible.
async fn resolve_version(
    project_id: &str,
    version_id: Option<&str>,
    loader: Option<&str>,
    mc: Option<&str>,
) -> Result<modrinth::ModrinthVersion> {
    match version_id {
        Some(id) => modrinth::version_by_id(id).await,
        None => modrinth::best_version(project_id, loader, mc).await,
    }
}

/// Which projects are already present in the instance.
async fn installed_project_ids(mods_dir: &Path) -> Result<HashSet<String>> {
    Ok(known_mods(mods_dir)
        .await?
        .into_iter()
        .map(|k| k.version.project_id)
        .filter(|id| !id.is_empty())
        .collect())
}

/// Lists what a mod version needs, marking what is already installed.
#[tauri::command]
pub async fn mod_dependencies(
    instance_id: String,
    project_id: String,
    version_id: Option<String>,
) -> Result<Vec<ModDependency>> {
    let (_dirs, mods_dir, loader, mc) = instances_and_mods(&instance_id)?;
    let version = resolve_version(
        &project_id,
        version_id.as_deref(),
        loader.as_deref(),
        mc.as_deref(),
    )
    .await?;
    let present = installed_project_ids(&mods_dir).await?;

    let mut out: Vec<ModDependency> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for dep in &version.dependencies {
        // Embedded dependencies ship inside the jar; nothing to install.
        if dep.dependency_type == "embedded" {
            continue;
        }
        let Some(dep_project) = dep.project_id.clone() else {
            continue;
        };
        if !seen.insert(dep_project.clone()) {
            continue;
        }
        let project = modrinth::project(&dep_project).await.ok();
        out.push(ModDependency {
            title: project
                .as_ref()
                .map(|p| p.title.clone())
                .unwrap_or_else(|| dep_project.clone()),
            icon_url: project.as_ref().and_then(|p| p.icon_url.clone()),
            installed: present.contains(&dep_project),
            dependency_type: dep.dependency_type.clone(),
            version_id: dep.version_id.clone(),
            project_id: dep_project,
        });
    }

    Ok(out)
}

/// Installs a mod plus its missing dependencies in one go.
#[tauri::command]
pub async fn install_mod_with_deps(
    instance_id: String,
    project_id: String,
    version_id: Option<String>,
    include_optional: Option<bool>,
) -> Result<InstallWithDepsReport> {
    let (_dirs, mods_dir, loader, mc) = instances_and_mods(&instance_id)?;
    let optional = include_optional.unwrap_or(false);

    let version = resolve_version(
        &project_id,
        version_id.as_deref(),
        loader.as_deref(),
        mc.as_deref(),
    )
    .await?;

    let mut installed = Vec::new();
    let mut skipped = Vec::new();

    installed.push(modrinth::install_version(&mods_dir, &version).await?);

    let present = installed_project_ids(&mods_dir).await?;
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(project_id.clone());

    for dep in &version.dependencies {
        let wanted = dep.dependency_type == "required"
            || (optional && dep.dependency_type == "optional");
        if !wanted {
            continue;
        }
        let Some(dep_project) = dep.project_id.clone() else {
            continue;
        };
        if !seen.insert(dep_project.clone()) || present.contains(&dep_project) {
            continue;
        }

        let resolved = resolve_version(
            &dep_project,
            dep.version_id.as_deref(),
            loader.as_deref(),
            mc.as_deref(),
        )
        .await;
        match resolved {
            Ok(dep_version) => match modrinth::install_version(&mods_dir, &dep_version).await {
                Ok(name) => installed.push(name),
                Err(_) => skipped.push(dep_project),
            },
            Err(_) => skipped.push(dep_project),
        }
    }

    if installed.is_empty() {
        return Err(NimbusError::Invalid(
            "Не удалось установить мод".to_owned(),
        ));
    }

    Ok(InstallWithDepsReport { installed, skipped })
}
