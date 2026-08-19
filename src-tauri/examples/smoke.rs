//! Headless acceptance runner: installs a version, launches it offline and
//! verifies the game process stays alive long enough to reach the main menu.
//!
//! Usage: cargo run --release --example smoke -- <version_id> [alive_secs]

use std::time::Duration;

use nimbus_lib::{assets, download, instance, java, launcher, libraries, natives, paths, version};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let version_id = args.next().expect("usage: smoke <version_id> [alive_secs]");
    let alive_secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(25);

    paths::ensure_all().expect("ensure dirs");

    let shared_dir = paths::shared_dir().unwrap();
    let instances_dir = paths::instances_dir().unwrap();
    let runtimes_dir = paths::runtimes_dir().unwrap();
    let libraries_root = shared_dir.join("libraries");
    let assets_root = assets::assets_root_path(&shared_dir);

    // ── Install ────────────────────────────────────────────────────────────
    println!("[smoke] fetching metadata for {version_id}");
    let meta = version::fetch_version_meta(&version_id)
        .await
        .expect("metadata");

    let java_major = meta
        .java_version
        .as_ref()
        .map(|jv| jv.major_version)
        .unwrap_or(8);
    println!("[smoke] resolving Java {java_major}");
    let javaw = java::resolve_java(java_major, &runtimes_dir)
        .await
        .expect("java");
    println!("[smoke] java: {}", javaw.display());

    let client_jar = version::client_jar_path(&version_id).unwrap();
    if let Some(dl) = meta.downloads.as_ref().map(|d| d.client.clone()) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move { while rx.recv().await.is_some() {} });
        println!("[smoke] downloading client jar ({} bytes)", dl.size);
        download::download_one(
            download::DownloadTask {
                url: dl.url.clone(),
                dest: client_jar.clone(),
                hash: Some(download::ExpectedHash::Sha1(dl.sha1.clone())),
                size: Some(dl.size),
            },
            tx,
        )
        .await
        .expect("client jar");
    }

    let resolved = libraries::resolve_libraries(&meta.libraries, &libraries_root);
    let tasks: Vec<download::DownloadTask> = resolved
        .iter()
        .filter(|l| !l.url.is_empty())
        .map(|l| download::DownloadTask {
            url: if l.url.starts_with("http") {
                l.url.clone()
            } else {
                format!("https://libraries.minecraft.net/{}", l.rel_path)
            },
            dest: libraries_root.join(&l.rel_path),
            hash: if l.sha1.is_empty() {
                None
            } else {
                Some(download::ExpectedHash::Sha1(l.sha1.clone()))
            },
            size: if l.size > 0 { Some(l.size) } else { None },
        })
        .collect();
    println!("[smoke] downloading {} libraries", tasks.len());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    download::download_many(tasks, tx).await.expect("libraries");

    let inst = instance::create(
        &instances_dir,
        format!("smoke-{version_id}"),
        version_id.clone(),
        None,
        None,
    )
    .expect("instance");
    let game_dir = inst.game_dir(&instances_dir);
    tokio::fs::create_dir_all(&game_dir).await.unwrap();

    println!("[smoke] downloading assets");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    assets::install_assets(&meta, &game_dir, &assets_root, tx)
        .await
        .expect("assets");

    // ── Launch ─────────────────────────────────────────────────────────────
    let natives_dir = inst.natives_dir(&instances_dir);
    tokio::fs::create_dir_all(&natives_dir).await.unwrap();
    natives::extract_natives(&resolved, &libraries_root, &natives_dir).expect("natives");

    let classpath = libraries::build_classpath(&resolved, &libraries_root, Some(&client_jar));
    let username = "SmokeTester";
    let uuid = launcher::offline_uuid(username);
    let asset_index_id = meta
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .unwrap_or_else(|| version_id.clone());

    let placeholders = launcher::Placeholders {
        auth_player_name: username.to_owned(),
        auth_uuid: uuid,
        auth_access_token: "0".to_owned(),
        auth_xuid: String::new(),
        user_type: "legacy".to_owned(),
        version_name: version_id.clone(),
        version_type: meta.kind.as_deref().unwrap_or("release").to_owned(),
        game_directory: game_dir.to_string_lossy().into_owned(),
        assets_root: assets_root.to_string_lossy().into_owned(),
        assets_index_name: asset_index_id,
        classpath,
        modulepath: String::new(),
        library_directory: libraries_root.to_string_lossy().into_owned(),
        classpath_separator: ";".to_owned(),
        natives_directory: natives_dir.to_string_lossy().into_owned(),
        launcher_name: "NimbusClient".to_owned(),
        launcher_version: "smoke".to_owned(),
        clientid: String::new(),
        resolution_width: None,
        resolution_height: None,
    };

    let cfg = launcher::LaunchConfig {
        java: javaw,
        jvm_prefix: vec![],
        aikar_flags: false,
        memory_mib: 2048,
        fullscreen: false,
        nimbus: None,
        authlib_injector: None,
        placeholders,
    };

    let args = launcher::build_command(&meta, &cfg).expect("args");
    println!("[smoke] launching: {} args", args.len());
    let spawned = launcher::spawn_game(&cfg.java, &args, &game_dir).expect("spawn");
    println!("[smoke] pid {}", spawned.pid);

    // The game must stay alive for `alive_secs`; an instant exit means a
    // classpath/args failure.
    let mut child = spawned.child;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_task = tokio::spawn(read_pipe(stdout));
    let err_task = tokio::spawn(read_pipe(stderr));
    match tokio::time::timeout(Duration::from_secs(alive_secs), child.wait()).await {
        Ok(status) => {
            let out = out_task.await.unwrap_or_default();
            let err = err_task.await.unwrap_or_default();
            eprintln!("[smoke] FAIL: game exited early: {status:?}");
            eprintln!("[smoke] --- stdout tail ---\n{}", tail(&out, 40));
            eprintln!("[smoke] --- stderr tail ---\n{}", tail(&err, 40));
            std::process::exit(1);
        }
        Err(_) => {
            println!("[smoke] OK: alive after {alive_secs}s, killing");
            let _ = child.kill().await;
        }
    }
}

async fn read_pipe<R: tokio::io::AsyncRead + Unpin>(pipe: Option<R>) -> String {
    use tokio::io::AsyncReadExt;
    let Some(mut pipe) = pipe else {
        return String::new();
    };
    let mut buf = Vec::new();
    let _ = pipe.read_to_end(&mut buf).await;
    String::from_utf8_lossy(&buf).into_owned()
}

fn tail(s: &str, lines: usize) -> String {
    let all: Vec<&str> = s.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}
