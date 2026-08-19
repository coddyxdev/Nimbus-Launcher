//! Downloads and caches `authlib-injector.jar`.
//!
//! It is the open-source Java agent that patches the game's authentication
//! library at runtime to point every Yggdrasil call -- login validation,
//! multiplayer session join, and skin/cape lookups -- at a third-party API
//! instead of Mojang's. Ely.by (`ely.rs`) is the API this launcher wires it
//! to; the agent itself is generic and maintained independently at
//! <https://github.com/yushijinhun/authlib-injector>.
//!
//! Without this agent an Ely.by account still launches (right username,
//! right UUID) but the JVM keeps talking to Mojang underneath, which
//! rejects an Ely.by token outright -- no skin, no working multiplayer join.

use std::path::PathBuf;

use serde::Deserialize;

use crate::download::{self, DownloadTask, ExpectedHash};
use crate::error::{NimbusError, Result};
use crate::paths;

/// Publishes every released build with its checksum, so the newest one can
/// always be found without hardcoding a version here.
const ARTIFACTS_URL: &str = "https://authlib-injector.yushi.moe/artifacts.json";

#[derive(Deserialize)]
struct ArtifactsResponse {
    artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
struct Artifact {
    build_number: u64,
    download_url: String,
    checksums: Checksums,
}

#[derive(Deserialize)]
struct Checksums {
    sha256: String,
}

fn install_dir() -> Result<PathBuf> {
    Ok(paths::shared_dir()?.join("authlib-injector"))
}

/// Downloads the latest authlib-injector build if it is not already cached
/// (the build number is baked into the file name, so a previous download is
/// simply reused), and returns its path.
pub async fn ensure_available() -> Result<PathBuf> {
    let resp: ArtifactsResponse = download::client()
        .get(ARTIFACTS_URL)
        .send()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;

    let latest = resp
        .artifacts
        .into_iter()
        .max_by_key(|a| a.build_number)
        .ok_or_else(|| {
            NimbusError::Invalid("authlib-injector: сервер вернул пустой список сборок".to_owned())
        })?;

    let dir = install_dir()?;
    tokio::fs::create_dir_all(&dir).await?;
    let dest = dir.join(format!("authlib-injector-{}.jar", latest.build_number));

    // Progress is not surfaced anywhere for this: the jar is a few hundred
    // KB and downloads far faster than a user could watch a bar for.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    download::download_one(
        DownloadTask {
            url: latest.download_url,
            dest: dest.clone(),
            hash: Some(ExpectedHash::Sha256(latest.checksums.sha256)),
            size: None,
        },
        tx,
    )
    .await?;

    Ok(dest)
}
