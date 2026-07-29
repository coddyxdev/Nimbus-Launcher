//! .mrpack (Modrinth modpack) import.
//!
//! A `.mrpack` file is a zip containing `modrinth.index.json` (name, MC
//! version, loader dependency, and the list of files to fetch by URL/hash)
//! plus an `overrides/` (and optionally `client-overrides/`) folder that is
//! copied verbatim into the game directory.
//!
//! Import reuses the entire vanilla/loader install pipeline from
//! [`super::install::install_version`] for the base game, then layers the
//! modpack's specific files and overrides on top.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::{AppHandle, Emitter};

use crate::download;
use crate::error::{NimbusError, Result};
use crate::instance::{self, Instance};
use crate::paths;

use super::shared::InstallProgress;

fn emit(app: &AppHandle, progress: InstallProgress) {
    let _ = app.emit("install:progress", progress);
}

#[derive(Debug, Deserialize)]
struct MrpackIndex {
    #[serde(rename = "formatVersion")]
    format_version: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    dependencies: HashMap<String, String>,
    #[serde(default)]
    files: Vec<MrpackFile>,
}

#[derive(Debug, Deserialize)]
struct MrpackFile {
    path: String,
    #[serde(default)]
    hashes: MrpackHashes,
    #[serde(default)]
    env: Option<MrpackEnv>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    file_size: u64,
}

#[derive(Debug, Deserialize, Default)]
struct MrpackHashes {
    #[serde(default)]
    sha1: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MrpackEnv {
    #[serde(default)]
    client: Option<String>,
}

/// Maps an `.mrpack` dependency key to the launcher's loader name.
fn dep_to_loader(key: &str) -> Option<&'static str> {
    match key {
        "fabric-loader" => Some("fabric"),
        "quilt-loader" => Some("quilt"),
        "forge" => Some("forge"),
        "neoforge" => Some("neoforge"),
        _ => None,
    }
}

/// Hosts a `.mrpack` is allowed to download from.
///
/// The Modrinth specification restricts modpack downloads to these domains.
/// Without the check an untrusted modpack could pull an arbitrary jar from any
/// server straight into `mods/`, where the game would then execute it.
const ALLOWED_DOWNLOAD_HOSTS: [&str; 5] = [
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "objects.githubusercontent.com",
    "gitlab.com",
];

fn download_host_allowed(url: &str) -> bool {
    let rest = match url.split_once("://") {
        Some(("https", rest)) => rest,
        // Plain HTTP is refused outright: the file would be unauthenticated.
        _ => return false,
    };
    let host = rest
        .split('/')
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    ALLOWED_DOWNLOAD_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

/// Joins `rel` onto `base`, rejecting traversal and absolute paths so a
/// malicious `.mrpack` cannot write outside the instance directory.
fn safe_join(base: &Path, rel: &str) -> Result<PathBuf> {
    let has_drive_letter = rel.len() >= 2 && rel.as_bytes()[1] == b':';
    let escapes = rel
        .split(['/', '\\'])
        .any(|segment| segment == ".." || segment.is_empty() && !rel.is_empty());
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.starts_with('\\')
        || has_drive_letter
        || escapes
    {
        return Err(NimbusError::Invalid(format!(
            "небезопасный путь в модпаке: {rel}"
        )));
    }
    Ok(base.join(rel.replace('\\', "/")))
}

type ParsedMrpack = (MrpackIndex, Vec<(String, Vec<u8>)>);

/// Parses the index and extracts override file bytes. Runs on the blocking
/// pool since `zip` is synchronous.
fn parse_mrpack(path: PathBuf) -> Result<ParsedMrpack> {
    let file = std::fs::File::open(&path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| NimbusError::Zip(e.to_string()))?;

    let index: MrpackIndex = {
        let mut entry = archive.by_name("modrinth.index.json").map_err(|_| {
            NimbusError::Invalid("Это не файл модпака Modrinth (.mrpack)".to_owned())
        })?;
        let mut body = String::new();
        entry.read_to_string(&mut body)?;
        serde_json::from_str(&body)?
    };

    let mut overrides = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| NimbusError::Zip(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let rel = name
            .strip_prefix("overrides/")
            .or_else(|| name.strip_prefix("client-overrides/"));
        if let Some(rel) = rel {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            overrides.push((rel.to_owned(), buf));
        }
    }

    Ok((index, overrides))
}

fn spawn_modpack_progress(
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
                            stage: "modpack-files".into(),
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

/// Imports a `.mrpack` modpack: installs the vanilla/loader base, downloads
/// every declared file, and writes the bundled overrides on top.
#[tauri::command]
pub async fn import_modpack(
    path: String,
    instance_name: Option<String>,
    app: AppHandle,
) -> Result<Instance> {
    let mrpack_path = PathBuf::from(&path);
    if !mrpack_path.is_file() {
        return Err(NimbusError::Invalid("Файл модпака не найден".to_owned()));
    }

    emit(&app, InstallProgress::stage("modpack-index", String::new()));
    let (index, overrides) = tokio::task::spawn_blocking(move || parse_mrpack(mrpack_path))
        .await
        .map_err(|e| NimbusError::Invalid(format!("modpack parse task failed: {e}")))??;

    if index.format_version != 1 {
        return Err(NimbusError::Invalid(format!(
            "Неподдерживаемая версия формата модпака: {}",
            index.format_version
        )));
    }

    let mc_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| NimbusError::Invalid("В модпаке не указана версия Minecraft".to_owned()))?;

    let (loader, loader_version) = index
        .dependencies
        .iter()
        .find_map(|(key, v)| dep_to_loader(key).map(|l| (l.to_owned(), v.clone())))
        .map(|(l, v)| (Some(l), Some(v)))
        .unwrap_or((None, None));

    let name = instance_name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| index.name.clone());

    // Reuses the whole vanilla/loader install pipeline: metadata, Java, client
    // jar, libraries, assets, instance bookkeeping and progress events.
    let inst =
        super::install::install_version(mc_version, name, loader, loader_version, app.clone())
            .await?;

    let instances_dir = paths::instances_dir()?;
    let game_dir = inst.game_dir(&instances_dir);

    // Modpack-specific files (mods, resourcepacks, configs...) declared by
    // hash and URL in modrinth.index.json.
    emit(&app, InstallProgress::stage("modpack-files", String::new()));
    let mut tasks = Vec::new();
    for f in &index.files {
        if let Some(env) = &f.env {
            if env.client.as_deref() == Some("unsupported") {
                continue;
            }
        }
        let Some(url) = f.downloads.first().cloned() else {
            continue;
        };
        if !download_host_allowed(&url) {
            return Err(NimbusError::Invalid(format!(
                "Модпак пытается скачать файл с недоверенного адреса: {url}"
            )));
        }
        // Without a hash there is nothing to verify the bytes against, and the
        // file lands in mods/ where the game will load it. Refuse instead.
        let Some(sha1) = f.hashes.sha1.clone() else {
            return Err(NimbusError::Invalid(format!(
                "В модпаке нет контрольной суммы для файла: {}",
                f.path
            )));
        };
        let dest = safe_join(&game_dir, &f.path)?;
        tasks.push(download::DownloadTask {
            url,
            dest,
            hash: Some(download::ExpectedHash::Sha1(sha1)),
            size: if f.file_size > 0 { Some(f.file_size) } else { None },
        });
    }
    let total_bytes: u64 = tasks.iter().filter_map(|t| t.size).sum();
    let tx = spawn_modpack_progress(&app, tasks.len() as u64, total_bytes);
    download::download_many(tasks, tx).await?;

    // Overrides ship as raw bytes bundled in the .mrpack and are written
    // as-is, taking priority over anything the base install created.
    emit(&app, InstallProgress::stage("overrides", String::new()));
    for (rel, bytes) in overrides {
        let dest = safe_join(&game_dir, &rel)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&dest, bytes).await?;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dep_keys_map_to_loader_names() {
        assert_eq!(dep_to_loader("fabric-loader"), Some("fabric"));
        assert_eq!(dep_to_loader("quilt-loader"), Some("quilt"));
        assert_eq!(dep_to_loader("forge"), Some("forge"));
        assert_eq!(dep_to_loader("neoforge"), Some("neoforge"));
        assert_eq!(dep_to_loader("minecraft"), None);
    }

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
