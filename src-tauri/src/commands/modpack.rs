//! `.mrpack` import, Modrinth-driven modpack install and update checks.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::download::{self, DownloadTask, ExpectedHash};
use crate::error::{NimbusError, Result};
use crate::instance::{self, Instance, ModpackSource};
use crate::modrinth;
use crate::paths;

#[derive(Debug, Deserialize)]
struct MrpackIndex {
    #[serde(rename = "formatVersion")]
    #[allow(dead_code)]
    format_version: u32,
    name: String,
    #[serde(default)]
    dependencies: std::collections::HashMap<String, String>,
    #[serde(default)]
    files: Vec<MrpackFile>,
}

#[derive(Debug, Deserialize)]
struct MrpackFile {
    path: String,
    hashes: MrpackHashes,
    #[serde(default)]
    env: Option<MrpackEnv>,
    downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    file_size: u64,
}

#[derive(Debug, Deserialize)]
struct MrpackHashes {
    sha1: String,
}

#[derive(Debug, Deserialize)]
struct MrpackEnv {
    #[serde(default)]
    client: Option<String>,
}

fn dep_to_loader(key: &str) -> Option<&'static str> {
    match key {
        "fabric-loader" => Some("fabric"),
        "quilt-loader" => Some("quilt"),
        "forge" => Some("forge"),
        "neoforge" => Some("neoforge"),
        _ => None,
    }
}

/// Hosts the launcher will download modpack files from. Keeps a malicious or
/// compromised `.mrpack`/API response from turning this feature into an
/// arbitrary-URL downloader.
const ALLOWED_DOWNLOAD_HOSTS: &[&str] = &[
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "objects.githubusercontent.com",
    "gitlab.com",
];

fn download_host_allowed(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .map(|host| {
            ALLOWED_DOWNLOAD_HOSTS
                .iter()
                .any(|allowed| host == *allowed)
        })
        .unwrap_or(false)
}

fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    crate::paths::safe_join(root, rel)
        .ok_or_else(|| NimbusError::Invalid(format!("небезопасный путь в архиве: {rel}")))
}

type ParsedMrpack = (MrpackIndex, Vec<(String, Vec<u8>)>);

/// Reads an `.mrpack` zip into its manifest plus the raw bytes of every file
/// under `overrides/`/`client-overrides/`.
fn parse_mrpack(path: &Path) -> Result<ParsedMrpack> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| NimbusError::Invalid(format!("Не удалось открыть .mrpack: {e}")))?;

    let index: MrpackIndex = {
        let entry = zip
            .by_name("modrinth.index.json")
            .map_err(|_| NimbusError::Invalid("В .mrpack нет modrinth.index.json".to_owned()))?;
        serde_json::from_reader(entry)?
    };

    let mut overrides = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| NimbusError::Zip(e.to_string()))?;
        let name = entry.name().to_owned();
        let rel = name
            .strip_prefix("overrides/")
            .or_else(|| name.strip_prefix("client-overrides/"));
        if let Some(rel) = rel {
            if entry.is_file() && !rel.is_empty() {
                use std::io::Read;
                let mut buf = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut buf)?;
                overrides.push((rel.to_owned(), buf));
            }
        }
    }

    Ok((index, overrides))
}

fn spawn_modpack_progress(app: &AppHandle, instance_id: &str) -> download::ProgressSender {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let app = app.clone();
    let instance_id = instance_id.to_owned();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let _ = app.emit("modpack://progress", (&instance_id, &event));
        }
    });
    tx
}

/// Downloads every declared file in the manifest and writes the override
/// tree into the instance's game directory. Shared by both the local
/// `.mrpack` import flow and the Modrinth-driven install flow so the actual
/// installation logic exists only once.
async fn apply_mrpack_contents(
    app: &AppHandle,
    instance_id: &str,
    game_dir: &Path,
    index: MrpackIndex,
    overrides: Vec<(String, Vec<u8>)>,
) -> Result<()> {
    let tx = spawn_modpack_progress(app, instance_id);

    let mods_dir = game_dir.join("mods");
    tokio::fs::create_dir_all(&mods_dir).await?;

    for file in index.files {
        if let Some(env) = &file.env {
            if env.client.as_deref() == Some("unsupported") {
                continue;
            }
        }
        let Some(url) = file.downloads.iter().find(|u| download_host_allowed(u)) else {
            return Err(NimbusError::Invalid(format!(
                "Файл {} ссылается на неразрешённый источник",
                file.path
            )));
        };
        let dest = safe_join(game_dir, &file.path)?;
        download::download_one(
            DownloadTask {
                url: url.clone(),
                dest,
                hash: Some(ExpectedHash::Sha1(file.hashes.sha1.clone())),
                size: if file.file_size > 0 {
                    Some(file.file_size)
                } else {
                    None
                },
            },
            tx.clone(),
        )
        .await?;
    }

    for (rel, bytes) in overrides {
        let dest = safe_join(game_dir, &rel)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&dest, &bytes).await?;
    }

    Ok(())
}

fn loader_from_index(index: &MrpackIndex) -> (Option<String>, Option<String>) {
    let loader = index
        .dependencies
        .keys()
        .find_map(|k| dep_to_loader(k))
        .map(str::to_owned);
    let loader_version = loader.as_ref().and_then(|l| {
        let dep_key = index
            .dependencies
            .keys()
            .find(|k| dep_to_loader(k) == Some(l.as_str()))?;
        index.dependencies.get(dep_key).cloned()
    });
    (loader, loader_version)
}

/// Imports a local `.mrpack` file into a new instance.
///
/// `project_id`/`version_id` are set when this import originated from
/// [`install_modpack_from_modrinth`], so the resulting instance remembers
/// where it came from and [`check_modpack_update`] can later look for newer
/// versions. They are `None` for a manually picked `.mrpack` file.
#[tauri::command]
pub async fn import_modpack(
    app: AppHandle,
    path: String,
    instance_name: Option<String>,
    project_id: Option<String>,
    version_id: Option<String>,
) -> Result<Instance> {
    let mrpack_path = PathBuf::from(&path);
    let (index, overrides) = tokio::task::spawn_blocking({
        let mrpack_path = mrpack_path.clone();
        move || parse_mrpack(&mrpack_path)
    })
    .await
    .map_err(|e| NimbusError::Invalid(format!("import task failed: {e}")))??;

    let (loader, loader_version) = loader_from_index(&index);
    let mc_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| {
            NimbusError::Invalid("В манифесте не указана версия Minecraft".to_owned())
        })?;

    let instances_dir = paths::instances_dir()?;
    let name = instance_name.unwrap_or_else(|| index.name.clone());
    let mut inst =
        super::install::install_version(mc_version, name, loader, loader_version, app.clone())
            .await?;

    if project_id.is_some() || version_id.is_some() {
        let source = match (&project_id, &version_id) {
            (Some(p), Some(v)) => Some(ModpackSource {
                project_id: p.clone(),
                version_id: v.clone(),
            }),
            _ => None,
        };
        inst = instance::set_modpack_source(&instances_dir, &inst.id, source)?;
    }

    let game_dir = inst.game_dir(&instances_dir);
    tokio::fs::create_dir_all(&game_dir).await?;

    apply_mrpack_contents(&app, &inst.id, &game_dir, index, overrides).await?;

    Ok(inst)
}

/// Downloads a Modrinth version's primary `.mrpack` file to a temp path.
async fn download_mrpack_to_temp(version: &modrinth::ModrinthVersion) -> Result<PathBuf> {
    let file = version
        .primary_file()
        .ok_or_else(|| NimbusError::Invalid("У этой версии модпака нет файлов".to_owned()))?;
    if !download_host_allowed(&file.url) {
        return Err(NimbusError::Invalid(
            "Файл модпака ссылается на неразрешённый источник".to_owned(),
        ));
    }

    let tmp_dir = std::env::temp_dir().join("nimbus-modpacks");
    tokio::fs::create_dir_all(&tmp_dir).await?;
    let dest = tmp_dir.join(format!("{}-{}.mrpack", version.id, file.filename));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    download::download_one(
        DownloadTask {
            url: file.url.clone(),
            dest: dest.clone(),
            hash: file.hashes.sha1.clone().map(ExpectedHash::Sha1),
            size: if file.size > 0 { Some(file.size) } else { None },
        },
        tx,
    )
    .await?;

    Ok(dest)
}

/// Searches Modrinth modpacks, for the "install from Modrinth" flow.
#[tauri::command]
pub async fn modrinth_search_modpacks(
    query: String,
    loader: Option<String>,
    mc_version: Option<String>,
    offset: Option<u32>,
    sort: Option<String>,
) -> Result<modrinth::ModrinthSearchPage> {
    modrinth::search_modpacks(
        &query,
        loader.as_deref(),
        mc_version.as_deref(),
        30,
        offset.unwrap_or(0),
        sort.as_deref(),
    )
    .await
}

/// Downloads the newest compatible version of a Modrinth modpack and imports
/// it as a new instance whose `modpackSource` is recorded for later updates.
#[tauri::command]
pub async fn install_modpack_from_modrinth(
    app: AppHandle,
    project_id: String,
    instance_name: Option<String>,
) -> Result<Instance> {
    let version = modrinth::best_version(&project_id, None, None).await?;
    let mrpack_path = download_mrpack_to_temp(&version).await?;

    let result = import_modpack(
        app,
        mrpack_path.to_string_lossy().to_string(),
        instance_name,
        Some(project_id),
        Some(version.id.clone()),
    )
    .await;

    let _ = tokio::fs::remove_file(&mrpack_path).await;
    result
}

/// Whether a newer Modrinth version exists for an instance's modpack source.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackUpdateInfo {
    pub has_update: bool,
    pub current_version_id: String,
    pub latest_version_id: String,
    pub latest_version_name: String,
}

/// Checks whether the instance's Modrinth modpack has a newer version.
///
/// Returns an error if the instance was not installed from Modrinth (no
/// `modpackSource`), since there is then nothing to compare against.
#[tauri::command]
pub async fn check_modpack_update(instance_id: String) -> Result<ModpackUpdateInfo> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let source = inst.modpack_source.ok_or_else(|| {
        NimbusError::Invalid(
            "Этот модпак не был установлен через Modrinth, обновление недоступно".to_owned(),
        )
    })?;

    let latest = modrinth::best_version(&source.project_id, None, None).await?;
    Ok(ModpackUpdateInfo {
        has_update: latest.id != source.version_id,
        current_version_id: source.version_id,
        latest_version_id: latest.id,
        latest_version_name: if latest.name.is_empty() {
            latest.version_number
        } else {
            latest.name
        },
    })
}

/// Downloads and applies the newest Modrinth version over an existing
/// instance's game directory, then records the new version as the source.
///
/// This only adds/overwrites files declared by the new manifest; it does not
/// remove mods that existed only in the previous version. Files the user
/// added manually are left untouched either way.
#[tauri::command]
pub async fn update_modpack(app: AppHandle, instance_id: String) -> Result<Instance> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let source = inst.modpack_source.clone().ok_or_else(|| {
        NimbusError::Invalid(
            "Этот модпак не был установлен через Modrinth, обновление недоступно".to_owned(),
        )
    })?;

    let latest = modrinth::best_version(&source.project_id, None, None).await?;
    let mrpack_path = download_mrpack_to_temp(&latest).await?;

    let (index, overrides) = tokio::task::spawn_blocking({
        let mrpack_path = mrpack_path.clone();
        move || parse_mrpack(&mrpack_path)
    })
    .await
    .map_err(|e| NimbusError::Invalid(format!("update task failed: {e}")))??;

    let game_dir = inst.game_dir(&instances_dir);
    apply_mrpack_contents(&app, &inst.id, &game_dir, index, overrides).await?;
    let _ = tokio::fs::remove_file(&mrpack_path).await;

    instance::set_modpack_source(
        &instances_dir,
        &inst.id,
        Some(ModpackSource {
            project_id: source.project_id,
            version_id: latest.id,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dep_keys_map_to_loader_names() {
        assert_eq!(dep_to_loader("fabric-loader"), Some("fabric"));
        assert_eq!(dep_to_loader("forge"), Some("forge"));
        assert_eq!(dep_to_loader("neoforge"), Some("neoforge"));
        assert_eq!(dep_to_loader("quilt-loader"), Some("quilt"));
        assert_eq!(dep_to_loader("minecraft"), None);
    }

    #[test]
    fn safe_join_accepts_normal_relative_paths() {
        let root = std::env::temp_dir();
        assert!(safe_join(&root, "config/mod.toml").is_ok());
    }

    #[test]
    fn safe_join_rejects_traversal_and_absolute_paths() {
        let root = std::env::temp_dir();
        assert!(safe_join(&root, "../../evil").is_err());
        assert!(safe_join(&root, "C:/Windows/System32/evil.dll").is_err());
    }

    #[test]
    fn download_host_allowlist_accepts_known_hosts_only() {
        assert!(download_host_allowed("https://cdn.modrinth.com/data/x.jar"));
        assert!(download_host_allowed(
            "https://github.com/foo/bar/releases/x.jar"
        ));
        assert!(!download_host_allowed("https://evil.example.com/x.jar"));
    }
}
