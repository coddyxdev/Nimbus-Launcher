//! Exporting an instance as a Modrinth `.mrpack` file.
//!
//! Import already exists (`modpack.rs`); without export a player can receive a
//! pack but never share one, which is the single cheapest growth feature a
//! launcher can have. The format is a zip containing `modrinth.index.json`
//! plus an `overrides/` tree:
//!
//! - Every mod that Modrinth recognises by its SHA-1 becomes an index entry
//!   with a download URL, so the archive stays a few hundred kilobytes.
//! - Everything else (hand-built jars, configs, resource packs, shaders) is
//!   copied into `overrides/` verbatim.
//!
//! Disabled mods (`*.jar.disabled`) are skipped on purpose: they are not part
//! of the working set the author is sharing.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::Digest as _;
use tauri::AppHandle;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::{NimbusError, Result};
use crate::instance::Instance;
use crate::{download, instance, modrinth, paths};

use super::shared::ensure_not_running;

/// Game directories worth carrying over in `overrides/`. `saves` is excluded:
/// worlds are personal, often gigabytes, and not what a shared pack is for.
const OVERRIDE_DIRS: [&str; 4] = ["config", "resourcepacks", "shaderpacks", "kubejs"];
/// Single files worth carrying over.
const OVERRIDE_FILES: [&str; 2] = ["options.txt", "servers.dat"];

fn zip_err(e: zip::result::ZipError) -> NimbusError {
    NimbusError::Zip(e.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexHashes {
    sha1: String,
    sha512: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexFile {
    /// Destination path inside the game directory, always `/`-separated.
    path: String,
    hashes: IndexHashes,
    downloads: Vec<String>,
    file_size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackIndex {
    format_version: u32,
    game: String,
    version_id: String,
    name: String,
    summary: Option<String>,
    files: Vec<IndexFile>,
    dependencies: HashMap<String, String>,
}

/// Result shown to the user after an export, so they can see what ended up
/// as a download link and what had to be bundled.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub path: String,
    /// Mods resolved to a Modrinth download.
    pub linked_mods: usize,
    /// Mods copied into `overrides/` because Modrinth did not know them.
    pub bundled_mods: usize,
    pub size_bytes: u64,
}

/// `{ "minecraft": "1.21", "fabric-loader": "0.16.0" }` — the dependency block
/// every `.mrpack` consumer reads to rebuild the instance.
fn dependencies(inst: &Instance) -> HashMap<String, String> {
    let mut deps = HashMap::new();
    let mc = inst
        .minecraft_version
        .clone()
        .unwrap_or_else(|| inst.version_id.clone());
    deps.insert("minecraft".to_owned(), mc);

    if let (Some(loader), Some(version)) = (inst.loader.as_deref(), inst.loader_version.clone()) {
        let key = match loader {
            "fabric" => Some("fabric-loader"),
            "quilt" => Some("quilt-loader"),
            "forge" => Some("forge"),
            "neoforge" => Some("neoforge"),
            _ => None,
        };
        if let Some(key) = key {
            deps.insert(key.to_owned(), version);
        }
    }
    deps
}

fn sha512_hex(path: &Path) -> Result<String> {
    use std::io::Read as _;
    let mut file = File::open(path)?;
    let mut hasher = sha2::Sha512::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Adds one file to the archive under `entry_name`.
fn add_file(zip: &mut ZipWriter<File>, path: &Path, entry_name: &str, options: SimpleFileOptions) -> Result<()> {
    zip.start_file(entry_name, options).map_err(zip_err)?;
    let mut f = File::open(path)?;
    std::io::copy(&mut f, zip)?;
    Ok(())
}

/// Recursively adds `dir` under `overrides/<prefix>/...`.
fn add_override_dir(
    zip: &mut ZipWriter<File>,
    base: &Path,
    dir: &Path,
    options: SimpleFileOptions,
) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            add_override_dir(zip, base, &path, options)?;
            continue;
        }
        let Ok(rel) = path.strip_prefix(base) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        add_file(zip, &path, &format!("overrides/{rel}"), options)?;
    }
    Ok(())
}

/// Exports an instance to `dest_path` as a `.mrpack`.
///
/// Refused while the instance is running: its files may be mid-write, and a
/// half-written jar would produce a pack that fails to install for everyone
/// who receives it.
#[tauri::command]
pub async fn export_mrpack(
    app: AppHandle,
    instance_id: String,
    dest_path: String,
    version_name: Option<String>,
) -> Result<ExportReport> {
    ensure_not_running(&app, &instance_id)?;

    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    let game_dir = inst.game_dir(&instances_dir);
    let mods_dir = inst.mods_dir(&instances_dir);

    // 1. Hash every enabled jar, then ask Modrinth which ones it knows.
    let mut jars: Vec<(PathBuf, String, u64)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !path.is_file() || !name.ends_with(".jar") {
                continue;
            }
            let sha1 = download::hash_file(&path, "sha1").await?;
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            jars.push((path, sha1, size));
        }
    }

    let hashes: Vec<String> = jars.iter().map(|(_, sha1, _)| sha1.clone()).collect();
    // A failed lookup is not fatal: everything simply lands in overrides/.
    let known = if hashes.is_empty() {
        HashMap::new()
    } else {
        modrinth::versions_by_hashes(&hashes)
            .await
            .unwrap_or_default()
    };

    let mut files = Vec::new();
    let mut bundled: Vec<PathBuf> = Vec::new();
    for (path, sha1, size) in &jars {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let download_url = known.get(sha1).and_then(|version| {
            version
                .files
                .iter()
                .find(|f| f.hashes.sha1.as_deref() == Some(sha1.as_str()))
                .or_else(|| version.files.iter().find(|f| f.primary))
                .map(|f| f.url.clone())
        });

        match download_url {
            Some(url) => files.push(IndexFile {
                path: format!("mods/{file_name}"),
                hashes: IndexHashes {
                    sha1: sha1.clone(),
                    sha512: sha512_hex(path)?,
                },
                downloads: vec![url],
                file_size: *size,
            }),
            None => bundled.push(path.clone()),
        }
    }

    let index = PackIndex {
        format_version: 1,
        game: "minecraft".to_owned(),
        version_id: version_name.unwrap_or_else(|| "1.0.0".to_owned()),
        name: inst.name.clone(),
        summary: None,
        files,
        dependencies: dependencies(&inst),
    };

    let linked_mods = index.files.len();
    let bundled_mods = bundled.len();
    let index_json = serde_json::to_vec_pretty(&index)?;
    let dest = PathBuf::from(&dest_path);

    // 2. Write the archive off the async runtime: zip + IO is blocking work.
    let size_bytes = tokio::task::spawn_blocking(move || -> Result<u64> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(&dest)?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        zip.start_file("modrinth.index.json", options)
            .map_err(zip_err)?;
        std::io::Write::write_all(&mut zip, &index_json)?;

        for path in bundled {
            if let Some(name) = path.file_name() {
                let name = name.to_string_lossy();
                add_file(&mut zip, &path, &format!("overrides/mods/{name}"), options)?;
            }
        }

        for dir_name in OVERRIDE_DIRS {
            let dir = game_dir.join(dir_name);
            if dir.is_dir() {
                add_override_dir(&mut zip, &game_dir, &dir, options)?;
            }
        }
        for file_name in OVERRIDE_FILES {
            let path = game_dir.join(file_name);
            if path.is_file() {
                add_file(&mut zip, &path, &format!("overrides/{file_name}"), options)?;
            }
        }

        zip.finish().map_err(zip_err)?;
        Ok(std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0))
    })
    .await
    .map_err(|err| NimbusError::Invalid(format!("export task failed: {err}")))??;

    Ok(ExportReport {
        path: dest_path,
        linked_mods,
        bundled_mods,
        size_bytes,
    })
}
