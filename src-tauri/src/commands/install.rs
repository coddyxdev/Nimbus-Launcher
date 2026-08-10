//! Version and loader installation, with progress events and cancellation.

use std::path::Path;

use tauri::{AppHandle, Emitter, Manager};

use crate::error::{NimbusError, Result};
use crate::instance::{self, Instance};
use crate::loader::{self, ModLoader};
use crate::version;
use crate::{assets, download, java, libraries, paths};

use super::shared::{
    library_url, validate_instance_name, CancelToken, InstallCancel, InstallProgress,
};

fn emit(app: &AppHandle, progress: InstallProgress) {
    let _ = app.emit("install:progress", progress);
}

/// Builds download tasks for every resolved library that has a URL.
fn library_tasks(
    resolved: &[libraries::ResolvedLib],
    libraries_root: &Path,
) -> Vec<download::DownloadTask> {
    resolved
        .iter()
        .filter(|l| !l.url.is_empty())
        .map(|l| download::DownloadTask {
            url: library_url(l),
            dest: libraries_root.join(&l.rel_path),
            hash: if l.sha1.is_empty() {
                None
            } else {
                Some(download::ExpectedHash::Sha1(l.sha1.clone()))
            },
            size: if l.size > 0 { Some(l.size) } else { None },
        })
        .collect()
}

/// Spawns a forwarder that turns raw download events into throttled
/// `install:progress` events for the `libraries` stage.
fn spawn_library_progress(
    app: &AppHandle,
    total_tasks: u64,
    total_bytes: u64,
) -> download::ProgressSender {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<download::ProgressEvent>();
    let app = app.clone();
    tokio::spawn(async move {
        let mut done: u64 = 0;
        let mut bytes_done: u64 = 0;
        while let Some(ev) = rx.recv().await {
            match ev {
                download::ProgressEvent::Finished { .. } => {
                    done += 1;
                    emit(
                        &app,
                        InstallProgress {
                            stage: "libraries".into(),
                            file: done.to_string(),
                            done,
                            total: total_tasks,
                            bytes_done,
                            bytes_total: total_bytes,
                        },
                    );
                }
                download::ProgressEvent::Bytes { delta, .. } => bytes_done += delta,
                _ => {}
            }
        }
    });
    tx
}

/// Requests cancellation of a running operation.
///
/// `key` is the operation key reported by the install commands (for example
/// `verify:<id>`). Without one, every running operation is cancelled, which is
/// what the single global cancel button in the UI does.
#[tauri::command]
pub fn cancel_install(app: AppHandle, key: Option<String>) -> Result<()> {
    let state = app.state::<InstallCancel>();
    match key.as_deref() {
        Some(key) => {
            state.request(key);
        }
        None => state.request_all(),
    }
    Ok(())
}

/// Downloads all files for `version_id`, creates an instance, and streams
/// `install:progress` events. Returns the created instance on success.
///
/// When `loader` and `loader_version` are provided, the instance is created
/// with a mod loader profile (Fabric/Quilt/Forge/NeoForge) that extends the
/// base Minecraft version via `inheritsFrom`.
///
/// The instance is written to disk with `installed: false` and only flipped to
/// `true` at the very end, so an interrupted or cancelled install is visible
/// as incomplete instead of looking ready to play.
#[tauri::command]
pub async fn install_version(
    version_id: String,
    instance_name: String,
    loader: Option<String>,
    loader_version: Option<String>,
    app: AppHandle,
) -> Result<Instance> {
    let instance_name = validate_instance_name(&instance_name)?;
    let shared_dir = paths::shared_dir()?;
    let instances_dir = paths::instances_dir()?;
    let runtimes_dir = paths::runtimes_dir()?;
    let libraries_root = shared_dir.join("libraries");
    let assets_root = assets::assets_root_path(&shared_dir);

    // Scoped to this instance name, so cancelling this install cannot abort a
    // verify or another install running at the same time. Dropped (and thus
    // deregistered) on every exit path.
    let cancel = CancelToken::begin(&app, &format!("install:{instance_name}"));

    // Determine the effective version ID and MC version.
    let (effective_version_id, minecraft_version) =
        if let (Some(ref ldr), Some(ref ldr_ver)) = (&loader, &loader_version) {
            let loader_enum = ModLoader::from_str(ldr)
                .ok_or_else(|| NimbusError::Invalid(format!("unknown loader: {ldr}")))?;
            let profile_id = loader::profile_id(&loader_enum, ldr_ver, &version_id);
            (profile_id, Some(version_id.clone()))
        } else {
            (version_id.clone(), None)
        };

    // ── 1. Version metadata ────────────────────────────────────────────────
    emit(&app, InstallProgress::stage("metadata", version_id.clone()));
    let meta = version::fetch_version_meta(&version_id).await?;
    cancel.check()?;

    // ── 1b. Loader profile ─────────────────────────────────────────────────
    if let (Some(ref ldr), Some(ref ldr_ver)) = (&loader, &loader_version) {
        emit(
            &app,
            InstallProgress::stage("loader", format!("{ldr} {ldr_ver}")),
        );
        let loader_enum = ModLoader::from_str(ldr)
            .ok_or_else(|| NimbusError::Invalid(format!("unknown loader: {ldr}")))?;
        loader::download_loader_profile(&loader_enum, &version_id, ldr_ver, None).await?;
        cancel.check()?;
    }

    // ── 2. Java ────────────────────────────────────────────────────────────
    let java_major = meta
        .java_version
        .as_ref()
        .map(|jv| jv.major_version)
        .unwrap_or(8);
    emit(
        &app,
        InstallProgress::stage("java", format!("Java {java_major}")),
    );
    java::resolve_java(java_major, &runtimes_dir).await?;
    cancel.check()?;

    // ── 3. Client jar ──────────────────────────────────────────────────────
    // Always the base MC version, never the loader profile id.
    let base_version_id = minecraft_version.as_deref().unwrap_or(&version_id);
    let client_jar = version::client_jar_path(base_version_id)?;
    if let Some(client_dl) = meta.downloads.as_ref().map(|d| d.client.clone()) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<download::ProgressEvent>();
        let app3 = app.clone();
        let vid = version_id.clone();
        let total = client_dl.size;
        tokio::spawn(async move {
            let mut done: u64 = 0;
            while let Some(ev) = rx.recv().await {
                if let download::ProgressEvent::Bytes { delta, .. } = ev {
                    done += delta;
                    emit(
                        &app3,
                        InstallProgress {
                            stage: "client".into(),
                            file: vid.clone(),
                            done,
                            total,
                            bytes_done: done,
                            bytes_total: total,
                        },
                    );
                }
            }
        });
        download::download_one(
            download::DownloadTask {
                url: client_dl.url.clone(),
                dest: client_jar.clone(),
                hash: Some(download::ExpectedHash::Sha1(client_dl.sha1.clone())),
                size: Some(client_dl.size),
            },
            tx,
        )
        .await?;
    }
    cancel.check()?;

    // ── 4. Libraries ───────────────────────────────────────────────────────
    emit(&app, InstallProgress::stage("libraries", String::new()));

    // The merged (inheritsFrom-resolved) profile lists both loader and vanilla
    // libraries; for vanilla installs the base metadata already has them all.
    let merged_meta = if minecraft_version.is_some() {
        version::fetch_any_version(&effective_version_id).await?
    } else {
        meta.clone()
    };

    let resolved_libs = libraries::resolve_libraries(&merged_meta.libraries, &libraries_root);
    let lib_tasks = library_tasks(&resolved_libs, &libraries_root);
    let total_lib_bytes: u64 = lib_tasks.iter().filter_map(|t| t.size).sum();
    let lib_tx = spawn_library_progress(&app, lib_tasks.len() as u64, total_lib_bytes);
    download::download_many(lib_tasks, lib_tx).await?;
    cancel.check()?;

    // ── 5. Instance + assets ───────────────────────────────────────────────
    let inst = {
        let created = instance::create(
            &instances_dir,
            instance_name,
            effective_version_id.clone(),
            loader,
            loader_version,
        )?;
        if let Some(ref mc_ver) = minecraft_version {
            let mut upd = created.clone();
            upd.minecraft_version = Some(mc_ver.clone());
            instance::save(&instances_dir, &upd)?;
            upd
        } else {
            created
        }
    };

    let game_dir = inst.game_dir(&instances_dir);
    tokio::fs::create_dir_all(&game_dir).await?;

    // Fabric needs the API jar for most mods to load at all.
    if inst.loader.as_deref() == Some("fabric") {
        if let Some(ref mc_ver) = inst.minecraft_version {
            emit(&app, InstallProgress::stage("fabric-api", "Fabric API"));
            let mods_dir = inst.mods_dir(&instances_dir);
            match loader::download_fabric_api(mc_ver, &mods_dir).await {
                Ok(name) => emit(
                    &app,
                    InstallProgress {
                        stage: "fabric-api".into(),
                        file: format!("Fabric API: {name}"),
                        done: 1,
                        total: 1,
                        bytes_done: 0,
                        bytes_total: 0,
                    },
                ),
                // Non-fatal: the instance is still playable without it.
                Err(e) => crate::nlog!("Fabric API download skipped: {e}"),
            }
        }
    }

    emit(&app, InstallProgress::stage("assets", String::new()));
    assets::install_assets(&meta, &game_dir, &assets_root, {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let app6 = app.clone();
        tokio::spawn(async move {
            // Asset installs emit tens of thousands of events; forwarding each
            // one to the WebView freezes the UI. Emit at most ~10/s plus one
            // final event with the exact totals.
            const EMIT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
            let mut done: u64 = 0;
            let mut bytes: u64 = 0;
            let mut last_emit = std::time::Instant::now();
            while let Some(ev) = rx.recv().await {
                match &ev {
                    download::ProgressEvent::Finished { .. } => done += 1,
                    download::ProgressEvent::Bytes { delta, .. } => bytes += delta,
                    _ => {}
                }
                if last_emit.elapsed() >= EMIT_INTERVAL {
                    last_emit = std::time::Instant::now();
                    emit(
                        &app6,
                        InstallProgress {
                            stage: "assets".into(),
                            file: String::new(),
                            done,
                            total: 0,
                            bytes_done: bytes,
                            bytes_total: 0,
                        },
                    );
                }
            }
            emit(
                &app6,
                InstallProgress {
                    stage: "assets".into(),
                    file: String::new(),
                    done,
                    total: done,
                    bytes_done: bytes,
                    bytes_total: bytes,
                },
            );
        });
        tx
    })
    .await?;

    // ── 6. Done ────────────────────────────────────────────────────────────
    // Cancelling after the files landed still leaves the instance marked
    // incomplete, so the user can retry and finish it later.
    cancel.check()?;
    let inst = instance::mark_installed(&instances_dir, &inst.id, true)?;

    emit(
        &app,
        InstallProgress {
            stage: "done".into(),
            file: inst.id.clone(),
            done: 1,
            total: 1,
            bytes_done: 0,
            bytes_total: 0,
        },
    );

    Ok(inst)
}

/// Installs a mod loader on an existing instance: downloads the loader
/// profile and its libraries, then updates the instance metadata.
#[tauri::command]
pub async fn install_loader(
    instance_id: String,
    loader: String,
    loader_version: String,
    app: AppHandle,
) -> Result<Instance> {
    let instances_dir = paths::instances_dir()?;
    let shared_dir = paths::shared_dir()?;
    let libraries_root = shared_dir.join("libraries");
    let mut inst = instance::load(&instances_dir, &instance_id)?;

    let cancel = CancelToken::begin(&app, &format!("loader:{instance_id}"));

    let loader_enum = ModLoader::from_str(&loader)
        .ok_or_else(|| NimbusError::Invalid(format!("unknown loader: {loader}")))?;

    let mc_version = inst
        .minecraft_version
        .clone()
        .unwrap_or_else(|| inst.version_id.clone());

    emit(
        &app,
        InstallProgress::stage("loader", format!("{loader} {loader_version}")),
    );

    let profile_id =
        loader::download_loader_profile(&loader_enum, &mc_version, &loader_version, None).await?;
    cancel.check()?;

    let merged_meta = version::fetch_any_version(&profile_id).await?;
    let resolved_libs = libraries::resolve_libraries(&merged_meta.libraries, &libraries_root);
    let lib_tasks = library_tasks(&resolved_libs, &libraries_root);
    let total_lib_bytes: u64 = lib_tasks.iter().filter_map(|t| t.size).sum();
    let lib_tx = spawn_library_progress(&app, lib_tasks.len() as u64, total_lib_bytes);
    download::download_many(lib_tasks, lib_tx).await?;
    cancel.check()?;

    inst.loader = Some(loader);
    inst.loader_version = Some(loader_version);
    inst.minecraft_version = Some(mc_version);
    inst.version_id = profile_id;
    inst.installed = Some(true);

    instance::save(&instances_dir, &inst)?;

    emit(
        &app,
        InstallProgress {
            stage: "done".into(),
            file: inst.id.clone(),
            done: 1,
            total: 1,
            bytes_done: 0,
            bytes_total: 0,
        },
    );

    Ok(inst)
}

/// Re-verifies every file of an instance and re-downloads whatever is missing
/// or has a wrong hash. Reuses the normal library pipeline, which skips files
/// whose hash already matches.
#[tauri::command]
pub async fn verify_instance(instance_id: String, app: AppHandle) -> Result<u64> {
    let instances_dir = paths::instances_dir()?;
    let shared_dir = paths::shared_dir()?;
    let libraries_root = shared_dir.join("libraries");
    let assets_root = assets::assets_root_path(&shared_dir);

    let inst = instance::load(&instances_dir, &instance_id)?;
    let cancel = CancelToken::begin(&app, &format!("verify:{instance_id}"));

    emit(
        &app,
        InstallProgress::stage("metadata", inst.version_id.clone()),
    );
    let meta = version::fetch_any_version(&inst.version_id).await?;

    // Client jar.
    let base_version_id = inst
        .minecraft_version
        .clone()
        .unwrap_or_else(|| inst.version_id.clone());
    let client_jar = version::client_jar_path(&base_version_id)?;
    let mut tasks: Vec<download::DownloadTask> = Vec::new();
    if let Some(client_dl) = meta.downloads.as_ref().map(|d| d.client.clone()) {
        tasks.push(download::DownloadTask {
            url: client_dl.url.clone(),
            dest: client_jar,
            hash: Some(download::ExpectedHash::Sha1(client_dl.sha1.clone())),
            size: Some(client_dl.size),
        });
    }

    // Libraries.
    let resolved = libraries::resolve_libraries(&meta.libraries, &libraries_root);
    tasks.extend(library_tasks(&resolved, &libraries_root));

    let checked = tasks.len() as u64;
    let total_bytes: u64 = tasks.iter().filter_map(|t| t.size).sum();
    let tx = spawn_library_progress(&app, checked, total_bytes);
    download::download_many(tasks, tx).await?;
    cancel.check()?;

    // Assets are hash-addressed, so install_assets is itself a verify pass.
    emit(&app, InstallProgress::stage("assets", String::new()));
    let game_dir = inst.game_dir(&instances_dir);
    let (atx, mut arx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move { while arx.recv().await.is_some() {} });
    assets::install_assets(&meta, &game_dir, &assets_root, atx).await?;

    // A successful verify also repairs the installed flag.
    instance::mark_installed(&instances_dir, &instance_id, true)?;

    emit(
        &app,
        InstallProgress {
            stage: "done".into(),
            file: instance_id,
            done: 1,
            total: 1,
            bytes_done: 0,
            bytes_total: 0,
        },
    );

    Ok(checked)
}
