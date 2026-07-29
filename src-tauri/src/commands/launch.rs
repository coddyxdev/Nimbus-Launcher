//! Launching, watching and killing the game process.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use crate::config;
use crate::error::{NimbusError, Result};
use crate::forge_install;
use crate::{account, assets, instance, java, launcher, libraries, natives, paths, presence, version};

use super::shared::{lock, GameHandle, RunningGames};

/// Dated game logs older than this are pruned on every launch.
const LOG_RETENTION_DAYS: u64 = 14;
/// Hard cap on retained dated log files, whatever their age.
const LOG_RETENTION_FILES: usize = 30;

#[derive(Debug, Serialize)]
pub struct LaunchResult {
    pid: u32,
}

/// Opens a log file for appending, or `None` when it cannot be created.
fn open_append(path: &PathBuf) -> Option<tokio::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
        .map(tokio::fs::File::from_std)
}

/// Deletes stale `game-YYYY-MM-DD.log` files.
///
/// Without this the log directory grows without bound: one file per day, none
/// of them ever removed. Both an age limit and a count limit are applied so a
/// burst of launches cannot outrun the age check.
fn prune_logs(log_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return;
    };

    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(LOG_RETENTION_DAYS * 86_400));

    let mut dated: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !(name.starts_with("game-") && name.ends_with(".log")) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);

        if let Some(cutoff) = cutoff {
            if modified < cutoff {
                let _ = std::fs::remove_file(&path);
                continue;
            }
        }
        dated.push((modified, path));
    }

    if dated.len() > LOG_RETENTION_FILES {
        // Oldest first, then drop everything above the cap.
        dated.sort_by_key(|(modified, _)| *modified);
        let excess = dated.len() - LOG_RETENTION_FILES;
        for (_, path) in dated.into_iter().take(excess) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Forwards one output stream to the frontend and to both log files.
///
/// stdout and stderr differ only by their label, so they share this helper
/// instead of duplicating ~40 lines of reader/writer plumbing.
fn pipe_stream<R>(
    app: AppHandle,
    instance_id: String,
    stream_label: &'static str,
    reader: Option<R>,
    log_path: PathBuf,
    dated_log_path: PathBuf,
) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let Some(reader) = reader else { return };
        let mut lines = tokio::io::BufReader::new(reader).lines();

        let mut latest = open_append(&log_path);
        let mut dated = open_append(&dated_log_path);

        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }
            let _ = app.emit(
                "game:output",
                serde_json::json!({
                    "instanceId": instance_id,
                    "line": line,
                    "stream": stream_label,
                }),
            );
            for file in [latest.as_mut(), dated.as_mut()].into_iter().flatten() {
                let _ = file.write_all(format!("{line}\n").as_bytes()).await;
                let _ = file.flush().await;
            }
        }
    })
}

/// Reports a pre-launch preparation step to the UI.
///
/// Forge/NeoForge instances run the installer's processors on first launch,
/// which can take tens of seconds. Without this the button just says "Запуск…"
/// and the launcher looks frozen.
fn emit_stage(app: &AppHandle, instance_id: &str, stage: &str, done: u64, total: u64) {
    let _ = app.emit(
        "launch:stage",
        serde_json::json!({
            "instanceId": instance_id,
            "stage": stage,
            "done": done,
            "total": total,
        }),
    );
}

#[tauri::command]
pub async fn launch_instance(instance_id: String, app: AppHandle) -> Result<LaunchResult> {
    let instances_dir = paths::instances_dir()?;
    let shared_dir = paths::shared_dir()?;
    let runtimes_dir = paths::runtimes_dir()?;
    let libraries_root = shared_dir.join("libraries");

    let inst = instance::load(&instances_dir, &instance_id)?;

    // Launching a half-installed instance produces confusing Java errors;
    // fail early with an actionable message instead.
    if !inst.is_installed() {
        return Err(NimbusError::Invalid(
            "Сборка установлена не полностью — запустите проверку файлов".to_string(),
        ));
    }
    if app.state::<RunningGames>().is_running(&instance_id) {
        return Err(NimbusError::Running);
    }

    emit_stage(&app, &instance_id, "metadata", 0, 4);

    // fetch_any_version resolves both Mojang and loader profiles.
    let meta = version::fetch_any_version(&inst.version_id).await?;

    let java_major = meta
        .java_version
        .as_ref()
        .map(|jv| jv.major_version)
        .unwrap_or(8);

    let cfg = config::load()?;

    emit_stage(&app, &instance_id, "java", 1, 4);
    // An explicit Java path from settings wins over auto-detection, but only
    // when it still exists on disk — otherwise fall back instead of failing.
    let javaw = match cfg.java_path.as_deref().map(PathBuf::from) {
        Some(path) if path.exists() => path,
        _ => java::resolve_java(java_major, &runtimes_dir).await?,
    };

    let resolved = libraries::resolve_libraries(&meta.libraries, &libraries_root);

    // For loader instances the client jar lives at the base MC version path.
    let client_jar_version = inst
        .minecraft_version
        .as_deref()
        .unwrap_or(&inst.version_id);
    let client_jar = version::client_jar_path(client_jar_version)?;
    let modulepath = libraries::build_modulepath(&resolved, &libraries_root);

    emit_stage(&app, &instance_id, "forge", 2, 4);
    // Forge/NeoForge 1.13+ ship no ready-made patched client: the installer
    // processors generate the srg/extra/patched jars locally. Without them FML
    // cannot find net/minecraft/client/Minecraft.class. Idempotent, cached.
    forge_install::ensure_processed(
        inst.loader.as_deref(),
        inst.loader_version.as_deref(),
        client_jar_version,
        &libraries_root,
        &client_jar,
        Some((&app, instance_id.as_str())),
    )
    .await?;

    let classpath = libraries::build_classpath(&resolved, &libraries_root, &client_jar);

    emit_stage(&app, &instance_id, "natives", 3, 4);
    let natives_dir = inst.natives_dir(&instances_dir);
    tokio::fs::create_dir_all(&natives_dir).await?;
    // Native extraction is synchronous zip work. block_in_place moves it off
    // the async scheduler so `game:output` and progress events keep flowing.
    tokio::task::block_in_place(|| {
        natives::extract_natives(&resolved, &libraries_root, &natives_dir)
    })?;

    let game_dir = inst.game_dir(&instances_dir);
    tokio::fs::create_dir_all(&game_dir).await?;

    // A signed-in Microsoft account wins over the offline nickname. The token
    // is refreshed here if needed, so an expired session fails with a clear
    // message instead of a rejected multiplayer join later on.
    let online = account::valid_account().await?;
    let (username, uuid, access_token, user_type) = match &online {
        Some(acc) => (
            acc.name.clone(),
            acc.uuid.clone(),
            acc.mc_access_token.clone(),
            "msa".to_owned(),
        ),
        None => {
            let name = cfg.offline_username.clone().unwrap_or_else(|| "Player".to_owned());
            let uuid = launcher::offline_uuid(&name);
            (name, uuid, "0".to_owned(), "legacy".to_owned())
        }
    };
    let assets_root = assets::assets_root_path(&shared_dir);
    let asset_index_id = meta
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .unwrap_or_else(|| {
            inst.minecraft_version
                .clone()
                .unwrap_or_else(|| inst.version_id.clone())
        });

    let placeholders = launcher::Placeholders {
        auth_player_name: username.clone(),
        auth_uuid: uuid,
        auth_access_token: access_token,
        auth_xuid: String::new(),
        user_type,
        version_name: inst.version_id.clone(),
        version_type: meta.kind.as_deref().unwrap_or("release").to_owned(),
        game_directory: game_dir.to_string_lossy().into_owned(),
        assets_root: assets_root.to_string_lossy().into_owned(),
        assets_index_name: asset_index_id,
        classpath,
        modulepath,
        library_directory: libraries_root.to_string_lossy().into_owned(),
        classpath_separator: ";".to_owned(),
        natives_directory: natives_dir.to_string_lossy().into_owned(),
        launcher_name: "NimbusClient".to_owned(),
        launcher_version: env!("CARGO_PKG_VERSION").to_owned(),
        clientid: String::new(),
        resolution_width: cfg.game_width.map(|w| w.to_string()),
        resolution_height: cfg.game_height.map(|h| h.to_string()),
    };

    // Per-instance overrides win over the global defaults.
    let launch_cfg = launcher::LaunchConfig {
        java: javaw,
        jvm_prefix: inst.jvm_args(&cfg.default_jvm_args),
        aikar_flags: inst.aikar_flags(cfg.default_aikar_flags),
        memory_mib: inst.memory_mib(cfg.default_memory_mib),
        fullscreen: cfg.game_fullscreen,
        placeholders,
    };

    let args = launcher::build_command(&meta, &launch_cfg)?;
    let spawned = launcher::spawn_game(&launch_cfg.java, &args, &game_dir)?;

    instance::touch_last_played(&instances_dir, &instance_id)?;
    emit_stage(&app, &instance_id, "done", 4, 4);

    // The watcher owns the Child handle; killing is requested over this channel
    // so we never operate on a bare (and potentially recycled) PID.
    let (kill_tx, mut kill_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    {
        let state = app.state::<RunningGames>();
        lock(&state.games).insert(
            instance_id.clone(),
            GameHandle {
                pid: spawned.pid,
                kill: kill_tx,
            },
        );
    }

    let pid = spawned.pid;
    let app2 = app.clone();
    let iid = instance_id.clone();
    let logs_dir = inst.logs_dir(&instances_dir);
    let instances_dir2 = instances_dir.clone();

    // Rich Presence details are assembled here, while the metadata is at hand.
    let rpc_enabled = cfg.discord_rpc;
    let rpc_name = inst.name.clone();
    let rpc_details = format!(
        "Minecraft {}{}",
        inst.minecraft_version.clone().unwrap_or_else(|| inst.version_id.clone()),
        inst.loader
            .as_deref()
            .map(|l| format!(" · {l}"))
            .unwrap_or_default()
    );

    tokio::spawn(async move {
        let started = std::time::Instant::now();
        if rpc_enabled {
            let epoch = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            presence::set_playing(rpc_name, rpc_details, epoch).await;
        }
        let mut child = spawned.child;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let log_dir = logs_dir;
        let _ = tokio::fs::create_dir_all(&log_dir).await;
        let prune_dir = log_dir.clone();
        let _ = tokio::task::spawn_blocking(move || prune_logs(&prune_dir)).await;

        // latest.log covers the current launch only; game-YYYY-MM-DD.log keeps
        // history.
        let timestamp = chrono::Local::now().format("%Y-%m-%d").to_string();
        let dated_log_path = log_dir.join(format!("game-{timestamp}.log"));
        let log_path = log_dir.join("latest.log");
        let _ = tokio::fs::write(&log_path, b"").await;

        let stdout_task = pipe_stream(
            app2.clone(),
            iid.clone(),
            "out",
            stdout,
            log_path.clone(),
            dated_log_path.clone(),
        );
        let stderr_task = pipe_stream(
            app2.clone(),
            iid.clone(),
            "err",
            stderr,
            log_path,
            dated_log_path,
        );

        // Either the game exits on its own, or the user asks us to stop it.
        // `Child::wait` is cancel-safe, so racing it in a loop is sound.
        let mut killed_by_user = false;
        let code = loop {
            tokio::select! {
                status = child.wait() => {
                    break status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
                }
                Some(()) = kill_rx.recv() => {
                    killed_by_user = true;
                    // taskkill takes down the whole tree (the JVM can spawn
                    // helpers). The PID is still valid: the child has not been
                    // reaped yet, so Windows cannot have reused it.
                    let _ = tokio::process::Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .output()
                        .await;
                }
            }
        };

        let _ = stdout_task.await;
        let _ = stderr_task.await;

        let played = started.elapsed().as_secs();
        let _ = instance::add_playtime(&instances_dir2, &iid, played);
        if rpc_enabled {
            presence::clear().await;
        }

        let state = app2.state::<RunningGames>();
        lock(&state.games).remove(&iid);

        // Single source of truth for game:exit; kill_instance stays silent so
        // the UI never sees the event twice.
        let _ = app2.emit(
            "game:exit",
            serde_json::json!({
                "instanceId": iid,
                "code": code,
                "killedByUser": killed_by_user,
                "playedSeconds": played,
            }),
        );
    });

    Ok(LaunchResult { pid })
}

/// Reports which Java the launcher would use, and whether it is a managed
/// runtime it downloaded itself or one already present on the system.
#[tauri::command]
pub async fn resolve_java(major_version: u32) -> Result<serde_json::Value> {
    let runtimes_dir = paths::runtimes_dir()?;
    let cfg = config::load()?;

    // Mirrors the resolution order used by launch_instance.
    if let Some(path) = cfg.java_path.as_deref().map(PathBuf::from) {
        if path.exists() {
            return Ok(serde_json::json!({
                "path": path.to_string_lossy(),
                "isManaged": false,
                "isOverride": true,
            }));
        }
    }

    let path = java::resolve_java(major_version, &runtimes_dir).await?;
    let is_managed = path.starts_with(&runtimes_dir);
    Ok(serde_json::json!({
        "path": path.to_string_lossy(),
        "isManaged": is_managed,
        "isOverride": false,
    }))
}

#[tauri::command]
pub async fn kill_instance(instance_id: String, app: AppHandle) -> Result<()> {
    let state = app.state::<RunningGames>();
    // The watcher performs the actual termination and owns the exit event.
    if state.request_kill(&instance_id).is_none() {
        return Err(NimbusError::Invalid("Игра не запущена".to_string()));
    }
    Ok(())
}
