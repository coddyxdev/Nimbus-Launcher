//! Importing instances from Prism Launcher and MultiMC.
//!
//! Their format is stable and open:
//!   <instance>/instance.cfg     INI-ish settings (name, memory, JVM args)
//!   <instance>/mmc-pack.json    component list: Minecraft version + loader
//!   <instance>/minecraft/       game directory (older MultiMC used .minecraft/)
//!
//! Import reuses the normal install pipeline for the base game and loader, then
//! copies the game directory on top. That way an imported instance is
//! indistinguishable from one created here — same shared libraries, same
//! verification, same launch path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{NimbusError, Result};
use crate::instance::{self, Instance, InstanceSettings};
use crate::paths;

/// Component uids used by Prism/MultiMC, mapped to our loader names.
const LOADER_UIDS: [(&str, &str); 4] = [
    ("net.fabricmc.fabric-loader", "fabric"),
    ("org.quiltmc.quilt-loader", "quilt"),
    ("net.minecraftforge", "forge"),
    ("net.neoforged", "neoforge"),
];

#[derive(Debug, Deserialize)]
struct MmcPack {
    #[serde(default)]
    components: Vec<MmcComponent>,
}

#[derive(Debug, Deserialize)]
struct MmcComponent {
    uid: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default, rename = "cachedVersion")]
    cached_version: Option<String>,
}

/// One importable instance found on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrismCandidate {
    /// Absolute path of the instance folder.
    pub path: String,
    pub name: String,
    pub minecraft_version: String,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
    /// Number of .jar files in mods/, purely informational.
    pub mods_count: u32,
    pub size_bytes: u64,
    /// Play time carried over from `totalTimePlayed`, in seconds.
    pub played_secs: u64,
}

/// Parses the `key=value` lines of instance.cfg. Sections are irrelevant here.
fn parse_cfg(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    map
}

/// Prism uses `minecraft/`, MultiMC used `.minecraft/`.
fn game_dir_of(instance_dir: &Path) -> Option<PathBuf> {
    for name in ["minecraft", ".minecraft"] {
        let candidate = instance_dir.join(name);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn count_jars(mods_dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".jar")
        })
        .count() as u32
}

/// Reads one instance folder, or `None` when it is not a valid instance.
fn read_candidate(dir: &Path) -> Option<PrismCandidate> {
    let cfg_text = std::fs::read_to_string(dir.join("instance.cfg")).ok()?;
    let cfg = parse_cfg(&cfg_text);

    let pack_text = std::fs::read_to_string(dir.join("mmc-pack.json")).ok()?;
    let pack: MmcPack = serde_json::from_str(&pack_text).ok()?;

    let version_of = |component: &MmcComponent| {
        component
            .version
            .clone()
            .or_else(|| component.cached_version.clone())
    };

    let minecraft_version = pack
        .components
        .iter()
        .find(|c| c.uid == "net.minecraft")
        .and_then(version_of)?;

    let (loader, loader_version) = pack
        .components
        .iter()
        .find_map(|c| {
            LOADER_UIDS
                .iter()
                .find(|(uid, _)| *uid == c.uid)
                .map(|(_, name)| ((*name).to_owned(), version_of(c)))
        })
        .map(|(name, version)| (Some(name), version))
        .unwrap_or((None, None));

    let name = cfg
        .get("name")
        .cloned()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            dir.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Импорт".to_owned())
        });

    let game_dir = game_dir_of(dir);
    let mods_count = game_dir
        .as_ref()
        .map(|g| count_jars(&g.join("mods")))
        .unwrap_or(0);
    let size_bytes = game_dir.as_ref().map(|g| instance::dir_size(g)).unwrap_or(0);
    let played_secs = cfg
        .get("totalTimePlayed")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    Some(PrismCandidate {
        path: dir.to_string_lossy().to_string(),
        name,
        minecraft_version,
        loader,
        loader_version,
        mods_count,
        size_bytes,
        played_secs,
    })
}

/// Lists importable instances inside a Prism/MultiMC `instances` folder.
///
/// The folder the user picks may itself be a single instance, which is handled
/// so they can drop either level of the tree in.
#[tauri::command]
pub async fn scan_prism_instances(root: String) -> Result<Vec<PrismCandidate>> {
    let root = PathBuf::from(root);
    if !root.is_dir() {
        return Err(NimbusError::Invalid("Папка не найдена".to_owned()));
    }

    tokio::task::spawn_blocking(move || {
        if let Some(single) = read_candidate(&root) {
            return vec![single];
        }
        let Ok(entries) = std::fs::read_dir(&root) else {
            return Vec::new();
        };
        let mut found: Vec<PrismCandidate> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| read_candidate(&e.path()))
            .collect();
        found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        found
    })
    .await
    .map_err(|e| NimbusError::Invalid(format!("сканирование не удалось: {e}")))
}

/// Recursively copies a directory tree. Unreadable entries are skipped rather
/// than aborting a mostly-successful import.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    let Ok(entries) = std::fs::read_dir(from) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let target = to.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            if let Err(err) = std::fs::copy(entry.path(), &target) {
                eprintln!(
                    "[nimbus] prism import: failed to copy {:?} -> {target:?} ({err})",
                    entry.path()
                );
            }
        }
    }
    Ok(())
}

/// Imports one Prism/MultiMC instance as a new Nimbus instance.
#[tauri::command]
pub async fn import_prism_instance(
    path: String,
    instance_name: Option<String>,
    app: AppHandle,
) -> Result<Instance> {
    let source = PathBuf::from(&path);
    let candidate = {
        let dir = source.clone();
        tokio::task::spawn_blocking(move || read_candidate(&dir))
            .await
            .map_err(|e| NimbusError::Invalid(format!("чтение не удалось: {e}")))?
            .ok_or_else(|| {
                NimbusError::Invalid(
                    "Это не похоже на сборку Prism/MultiMC: нет instance.cfg или mmc-pack.json"
                        .to_owned(),
                )
            })?
    };

    let name = instance_name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| candidate.name.clone());

    // Downloads the base game, loader, libraries and assets exactly like a
    // normal install, emitting the same progress events.
    let inst = super::install::install_version(
        candidate.minecraft_version.clone(),
        name,
        candidate.loader.clone(),
        candidate.loader_version.clone(),
        app,
    )
    .await?;

    let instances_dir = paths::instances_dir()?;
    let dest_game = inst.game_dir(&instances_dir);

    if let Some(source_game) = game_dir_of(&source) {
        tokio::task::spawn_blocking(move || copy_tree(&source_game, &dest_game))
            .await
            .map_err(|e| NimbusError::Invalid(format!("копирование не удалось: {e}")))??;
    }

    // Carry over the settings that have a direct equivalent.
    let cfg_text = std::fs::read_to_string(source.join("instance.cfg")).unwrap_or_default();
    let cfg = parse_cfg(&cfg_text);
    let memory_mib = cfg.get("MaxMemAlloc").and_then(|v| v.parse::<u32>().ok());
    let jvm_args: Option<Vec<String>> = cfg
        .get("JvmArgs")
        .map(|raw| raw.split_whitespace().map(str::to_owned).collect())
        .filter(|args: &Vec<String>| !args.is_empty());

    let mut imported = instance::load(&instances_dir, &inst.id)?;
    if memory_mib.is_some() || jvm_args.is_some() {
        imported.settings = Some(InstanceSettings {
            memory_mib,
            jvm_args,
            aikar_flags: None,
        });
    }
    if candidate.played_secs > 0 {
        imported.total_playtime_secs = Some(candidate.played_secs);
    }
    instance::save(&instances_dir, &imported)?;

    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_parsing_ignores_sections_and_comments() {
        let cfg = parse_cfg("[General]\n# note\nname = My Pack\nMaxMemAlloc=6144\n");
        assert_eq!(cfg.get("name").map(String::as_str), Some("My Pack"));
        assert_eq!(cfg.get("MaxMemAlloc").map(String::as_str), Some("6144"));
    }

    #[test]
    fn loader_uids_map_to_launcher_names() {
        let pack: MmcPack = serde_json::from_str(
            r#"{"components":[
                {"uid":"net.minecraft","version":"1.20.1"},
                {"uid":"net.fabricmc.fabric-loader","version":"0.15.11"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(pack.components.len(), 2);
        let found = pack
            .components
            .iter()
            .find_map(|c| LOADER_UIDS.iter().find(|(uid, _)| *uid == c.uid));
        assert_eq!(found.map(|(_, name)| *name), Some("fabric"));
    }
}
