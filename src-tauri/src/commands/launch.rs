//! Launching, watching and killing the game process.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufWriter};

use crate::config;
use crate::error::{NimbusError, Result};
use crate::forge_install;
use crate::{
    account, assets, instance, java, launcher, libraries, natives, paths, presence, version,
};

use super::shared::{lock, validate_instance_id, GameHandle, LaunchGuard, RunningGames};

/// Dated game logs older than this are pruned on every launch.
const LOG_RETENTION_DAYS: u64 = 14;
/// Hard cap on retained dated log files, whatever their age.
const LOG_RETENTION_FILES: usize = 30;
/// `game:output` lines are batched into at most one event (and one file
/// flush) per this many lines...
const OUTPUT_BATCH_LINES: usize = 50;
/// ...or per this much time, whichever comes first, so a slow trickle of
/// output is never held back waiting to fill a batch.
const OUTPUT_BATCH_INTERVAL: Duration = Duration::from_millis(100);

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
                if let Err(err) = std::fs::remove_file(&path) {
                    crate::nlog!("logs: failed to prune old log {path:?} ({err})");
                }
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
            if let Err(err) = std::fs::remove_file(&path) {
                crate::nlog!("logs: failed to prune excess log {path:?} ({err})");
            }
        }
    }
}

/// True when a Forge/NeoForge profile generates its own game jar instead of
/// running on the vanilla client.
///
/// Forge ≤1.12.2 patches the vanilla jar in place (launchwrapper), so the
/// vanilla client still is the game. From 1.13 on, the installer processors
/// produce a separate `-srg` client and the vanilla jar must not be on the
/// classpath.
fn forge_generates_client(mc_version: &str) -> bool {
    let mut parts = mc_version.split('.');
    match (
        parts.next().and_then(|p| p.parse::<u32>().ok()),
        parts.next().and_then(|p| p.parse::<u32>().ok()),
    ) {
        (Some(major), Some(minor)) => major > 1 || (major == 1 && minor >= 13),
        _ => false,
    }
}

/// One line of game output, tagged with the stream it came from.
type OutputLine = (&'static str, String);

/// Reads one output stream line by line and forwards it to the sink task.
///
/// `Lines::next_line` is deliberately NOT wrapped in a timeout here: it is not
/// cancel-safe, so the previous batching-by-timeout version silently dropped
/// whatever had already been read of a line whenever the batch window elapsed
/// mid-line. Batching now happens in the sink instead, around the cancel-safe
/// `Receiver::recv`.
fn spawn_stream_reader<R>(
    stream_label: &'static str,
    reader: Option<R>,
    tx: tokio::sync::mpsc::UnboundedSender<OutputLine>,
) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let Some(reader) = reader else { return };
        let mut lines = tokio::io::BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }
            // A closed receiver means the sink is gone; nothing left to do.
            if tx.send((stream_label, line)).is_err() {
                break;
            }
        }
    })
}

/// Single owner of both log files and of the `game:output` emit.
///
/// stdout and stderr used to open the same two files independently, so their
/// buffered writes could interleave inside a line and corrupt latest.log. Both
/// streams now funnel through this one task, which is also what batches them:
/// a modded Forge instance prints thousands of lines a second at startup, and
/// one IPC emit plus two file flushes per line stalls the WebView bridge, the
/// same failure mode `PROGRESS_INTERVAL` avoids for download progress.
fn spawn_output_sink(
    app: AppHandle,
    instance_id: String,
    log_path: PathBuf,
    dated_log_path: PathBuf,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<OutputLine>,
) -> tokio::task::JoinHandle<()> {
    async fn flush_batch(
        app: &AppHandle,
        instance_id: &str,
        batch: &mut Vec<OutputLine>,
        latest: &mut Option<BufWriter<tokio::fs::File>>,
        dated: &mut Option<BufWriter<tokio::fs::File>>,
    ) {
        if batch.is_empty() {
            return;
        }
        // Emitted per stream so the payload shape stays exactly what the
        // frontend expects, even though both streams share one batch now.
        for label in ["out", "err"] {
            let lines: Vec<&str> = batch
                .iter()
                .filter(|(stream, _)| *stream == label)
                .map(|(_, line)| line.as_str())
                .collect();
            if lines.is_empty() {
                continue;
            }
            let _ = app.emit(
                "game:output",
                serde_json::json!({
                    "instanceId": instance_id,
                    "lines": lines,
                    "stream": label,
                }),
            );
        }
        for file in [latest.as_mut(), dated.as_mut()].into_iter().flatten() {
            for (_, line) in batch.iter() {
                let _ = file.write_all(format!("{line}\n").as_bytes()).await;
            }
            let _ = file.flush().await;
        }
        batch.clear();
    }

    tokio::spawn(async move {
        // BufWriter absorbs the per-line write_all calls between batches, so
        // flush() runs once per batch instead of once per line.
        let mut latest = open_append(&log_path).map(BufWriter::new);
        let mut dated = open_append(&dated_log_path).map(BufWriter::new);
        let mut batch: Vec<OutputLine> = Vec::with_capacity(OUTPUT_BATCH_LINES);

        loop {
            match tokio::time::timeout(OUTPUT_BATCH_INTERVAL, rx.recv()).await {
                Ok(Some(item)) => {
                    batch.push(item);
                    if batch.len() >= OUTPUT_BATCH_LINES {
                        flush_batch(&app, &instance_id, &mut batch, &mut latest, &mut dated).await;
                    }
                }
                // Every sender dropped: both streams are done.
                Ok(None) => {
                    flush_batch(&app, &instance_id, &mut batch, &mut latest, &mut dated).await;
                    break;
                }
                // Batch window elapsed: flush so sparse output is never held
                // back waiting for the batch to fill up.
                Err(_) => {
                    flush_batch(&app, &instance_id, &mut batch, &mut latest, &mut dated).await;
                }
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
pub async fn launch_instance(
    instance_id: String,
    server: Option<String>,
    app: AppHandle,
) -> Result<LaunchResult> {
    validate_instance_id(&instance_id)?;
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
    // Claimed for the whole preparation instead of merely checked once:
    // everything below awaits (metadata, Java, Forge processors, natives), so a
    // plain check let two quick launches of the same instance both get through
    // and start two JVMs in one game directory. The guard is released on every
    // exit path, including errors.
    let _launch_slot = LaunchGuard::acquire(&app, &instance_id)?;

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

    // Forge/NeoForge 1.13+ bring their own game jar: the installer processors
    // above generated client-<mc>-<ts>-srg.jar into the shared libraries, and
    // the loader locates it as the `minecraft` module. The vanilla client must
    // stay OFF the classpath for them, or the boot layer sees two modules (the
    // vanilla jar named after its file, e.g. `1.21.1.jar` → `_1._21._1`, and
    // the loader's `minecraft`) exporting the same net.minecraft.* packages and
    // dies before FML even starts with
    // "Modules _1._21._1 and minecraft export package net.minecraft.server to
    // module mixin_synthetic". Vanilla, Fabric/Quilt and nimbus run on the
    // vanilla jar itself and keep it.
    let vanilla_client_needed = match inst.loader.as_deref() {
        Some("forge" | "neoforge") => !forge_generates_client(client_jar_version),
        _ => true,
    };
    let classpath = libraries::build_classpath(
        &resolved,
        &libraries_root,
        vanilla_client_needed.then(|| client_jar.as_path()),
    );

    emit_stage(&app, &instance_id, "natives", 3, 4);
    let natives_dir = inst.natives_dir(&instances_dir);
    tokio::fs::create_dir_all(&natives_dir).await?;
    // Native extraction is synchronous zip work. block_in_place moves it off
    // the async scheduler so `game:output` and progress events keep flowing.
    tokio::task::block_in_place(|| {
        natives::extract_natives(&resolved, &libraries_root, &natives_dir)
    })?;

    // Modern profiles (1.21.9+) point -Djna.tmpdir, -Dio.netty.native.workdir
    // and -Dorg.lwjgl.system.SharedLibraryExtractPath at subfolders of the
    // natives directory. JNA and netty do not create them, they just fail to
    // unpack, so make sure the folders exist before the JVM starts.
    for sub in ["java", "jna", "lwjgl", "netty"] {
        tokio::fs::create_dir_all(natives_dir.join(sub)).await?;
    }

    let game_dir = inst.game_dir(&instances_dir);
    tokio::fs::create_dir_all(&game_dir).await?;

    // A signed-in Microsoft account wins over the offline nickname. The token
    // is refreshed here if needed, so an expired session fails with a clear
    // message instead of a rejected multiplayer join later on.
    let online = account::valid_account().await?;
    let (username, uuid, access_token, user_type, xuid) = match &online {
        Some(acc) => (
            acc.name.clone(),
            acc.uuid.clone(),
            acc.mc_access_token.clone(),
            "msa".to_owned(),
            acc.xuid.clone(),
        ),
        None => {
            let name = cfg
                .offline_username
                .clone()
                .unwrap_or_else(|| "Player".to_owned());
            let uuid = launcher::offline_uuid(&name);
            (
                name,
                uuid,
                "0".to_owned(),
                "legacy".to_owned(),
                String::new(),
            )
        }
    };

    // ${auth_xuid} and ${clientid} are what the modern client sends with its
    // multiplayer session and telemetry requests; the official launcher fills
    // both. Empty for offline play, where there is no Xbox identity at all.
    let client_id = if online.is_some() {
        crate::auth::resolve_client_id(cfg.azure_client_id.as_deref()).unwrap_or_default()
    } else {
        String::new()
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
        auth_xuid: xuid,
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
        clientid: client_id,
        resolution_width: cfg.game_width.map(|w| w.to_string()),
        resolution_height: cfg.game_height.map(|h| h.to_string()),
    };

    // Nimbus client instances run the plain vanilla files with our own runtime
    // attached as a Java agent; every other loader is left untouched.
    let nimbus = match inst.loader.as_deref() {
        Some("nimbus") => {
            let agent_jar = paths::shared_dir()?.join("nimbus").join("nimbus-runtime.jar");
            if agent_jar.exists() {
                // Mappings are what let the agent find the game classes. A
                // failed download is not fatal: the agent already handles
                // starting without them, just with fewer hooks.
                let mappings = version::ensure_client_mappings(client_jar_version, &meta)
                    .await
                    .unwrap_or_default();
                Some(launcher::NimbusClient {
                    agent_jar,
                    game_version: client_jar_version.to_owned(),
                    mappings,
                    // Подробный лог агента включается переменной окружения:
                    // разработчику он нужен, игроку в логе только мешает.
                    debug: std::env::var_os("NIMBUS_CLIENT_DEBUG").is_some(),
                })
            } else {
                // The runtime is not installed yet: start plain vanilla rather
                // than refusing to launch.
                None
            }
        }
        _ => None,
    };

    // Per-instance overrides win over the global defaults.
    let launch_cfg = launcher::LaunchConfig {
        java: javaw,
        jvm_prefix: inst.jvm_args(&cfg.default_jvm_args),
        aikar_flags: inst.aikar_flags(cfg.default_aikar_flags),
        memory_mib: inst.memory_mib(cfg.default_memory_mib),
        fullscreen: cfg.game_fullscreen,
        nimbus,
        placeholders,
    };

    let mut args = launcher::build_command(&meta, &launch_cfg)?;

    // Joining a server straight from the launcher. 1.20+ profiles declare Quick
    // Play in their argument rules; older clients only understand the legacy
    // --server/--port pair, and passing the wrong one is silently ignored by
    // the game, so pick by what the profile actually supports.
    if let Some(address) = server
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let quick_play = serde_json::to_string(&meta)
            .map(|json| json.contains("quickPlayMultiplayer"))
            .unwrap_or(false);
        if quick_play {
            args.push("--quickPlayMultiplayer".to_owned());
            args.push(address.to_owned());
        } else {
            let (host, port) = super::servers::split_address(address);
            args.push("--server".to_owned());
            args.push(host);
            args.push("--port".to_owned());
            args.push(port.to_string());
        }
    }
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
        inst.minecraft_version
            .clone()
            .unwrap_or_else(|| inst.version_id.clone()),
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
            // Detached: Rich Presence talks to a Discord named pipe under a
            // global mutex and can block for seconds. Awaiting it here delayed
            // taking the child stdout/stderr, i.e. the first and most
            // interesting lines of a launch.
            tokio::spawn(async move {
                presence::set_playing(rpc_name, rpc_details, epoch).await;
            });
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

        // One sink owns both log files and the IPC emit; the readers only
        // forward lines into it, so their writes can no longer interleave.
        let (line_tx, line_rx) = tokio::sync::mpsc::unbounded_channel::<OutputLine>();
        let sink_task =
            spawn_output_sink(app2.clone(), iid.clone(), log_path, dated_log_path, line_rx);
        let stdout_task = spawn_stream_reader("out", stdout, line_tx.clone());
        let stderr_task = spawn_stream_reader("err", stderr, line_tx);

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
        // Both senders are gone now, so the sink flushes its tail and exits.
        let _ = sink_task.await;

        let played = started.elapsed().as_secs();
        let _ = instance::add_playtime(&instances_dir2, &iid, played);
        let state = app2.state::<RunningGames>();
        lock(&state.games).remove(&iid);

        if rpc_enabled {
            // Back to the idle "in launcher" status instead of clearing
            // entirely, since Nimbus Client is still open. Detached and moved
            // below the bookkeeping so a hung Discord pipe can never delay the
            // game:exit event and leave the UI showing a game that is gone.
            tokio::spawn(async {
                presence::set_idle().await;
            });
        }

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
    validate_instance_id(&instance_id)?;
    let state = app.state::<RunningGames>();
    // The watcher performs the actual termination and owns the exit event.
    if state.request_kill(&instance_id).is_none() {
        return Err(NimbusError::Invalid("Игра не запущена".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::forge_generates_client;

    #[test]
    fn forge_generates_client_from_1_13_on() {
        // ≤1.12.2: runs on the vanilla jar, it must stay on the classpath.
        assert!(!forge_generates_client("1.7.10"));
        assert!(!forge_generates_client("1.12.2"));
        // 1.13+ and everything newer: own srg client, vanilla jar excluded.
        assert!(forge_generates_client("1.13"));
        assert!(forge_generates_client("1.16.5"));
        assert!(forge_generates_client("1.20.1"));
        assert!(forge_generates_client("1.21.1"));
        assert!(forge_generates_client("1.21.11"));
        assert!(forge_generates_client("26.1"));
    }

    #[test]
    fn forge_generates_client_tolerates_unparseable_versions() {
        // Unparseable versions keep the vanilla jar (conservative).
        assert!(!forge_generates_client(""));
        assert!(!forge_generates_client("1"));
        assert!(!forge_generates_client("snapshot"));
    }
}
