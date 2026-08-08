//! Instance management: on-disk CRUD.
//!
//! Each instance lives at `instances/<id>/`:
//! - `instance.json` — metadata (id, name, version, timestamps).
//! - `game/`         — the `.minecraft` directory passed to the game.
//! - `natives/`      — extracted native libs (populated per-launch).
//!
//! Shared content (libraries, assets, version JARs) lives in `shared/`
//! and is never duplicated across instances.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::write_atomic;
use crate::error::{NimbusError, Result};

/// Per-instance overrides for launch settings. `None` means "use the global
/// value from config.json".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSettings {
    pub memory_mib: Option<u32>,
    pub jvm_args: Option<Vec<String>>,
    pub aikar_flags: Option<bool>,
}

/// Where a modpack instance's content came from, when it is linked to a
/// Modrinth project. Used only to check/apply modpack updates; instances
/// created from a locally picked `.mrpack` file with no known project id (or
/// not a modpack at all) simply have this as `None` and never report an
/// update.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackSource {
    pub project_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub version_id: String,
    /// Optional mod loader name: "fabric", "quilt", "forge", "neoforge".
    pub loader: Option<String>,
    pub loader_version: Option<String>,
    /// The base Minecraft version (e.g. "1.21"). When a mod loader is installed,
    /// `version_id` becomes the loader profile ID (e.g. `fabric-loader-0.16.0-1.21`)
    /// and `minecraft_version` stores the original MC version for resolving
    /// the client jar path and other MC-specific assets.
    pub minecraft_version: Option<String>,
    /// Unix timestamp (seconds) when the instance was created.
    pub created_at: u64,
    /// Unix timestamp of the last successful launch.
    pub last_played: Option<u64>,
    /// `false` while files are still downloading, or when an install failed
    /// half-way. Absent in instances written by launcher <= 1.1, which are
    /// treated as complete for backwards compatibility.
    #[serde(default)]
    pub installed: Option<bool>,
    /// Per-instance launch overrides.
    #[serde(default)]
    pub settings: Option<InstanceSettings>,
    /// Total time spent in game, in seconds. Accumulated on every exit.
    #[serde(default)]
    pub total_playtime_secs: Option<u64>,
    /// Set when this instance's mods/overrides came from a Modrinth modpack
    /// installed (or updated) through the launcher.
    #[serde(default)]
    pub modpack_source: Option<ModpackSource>,
}

impl Instance {
    /// Absolute path to this instance's root directory.
    pub fn dir(&self, instances_root: &Path) -> PathBuf {
        instances_root.join(&self.id)
    }

    /// The `.minecraft`-equivalent game directory.
    pub fn game_dir(&self, instances_root: &Path) -> PathBuf {
        self.dir(instances_root).join("game")
    }

    /// Extracted native libs directory. Created fresh each launch.
    pub fn natives_dir(&self, instances_root: &Path) -> PathBuf {
        self.dir(instances_root).join("natives")
    }

    /// Mod files directory. Created when the first mod is added.
    pub fn mods_dir(&self, instances_root: &Path) -> PathBuf {
        self.game_dir(instances_root).join("mods")
    }

    /// Log directory used by the launcher (not by the game itself).
    pub fn logs_dir(&self, instances_root: &Path) -> PathBuf {
        self.dir(instances_root).join("logs")
    }

    /// Legacy instances carry no `installed` flag and are assumed complete.
    pub fn is_installed(&self) -> bool {
        self.installed.unwrap_or(true)
    }

    /// Effective launch settings: per-instance override, else the global value.
    pub fn memory_mib(&self, global: u32) -> u32 {
        self.settings
            .as_ref()
            .and_then(|s| s.memory_mib)
            .unwrap_or(global)
    }

    pub fn jvm_args(&self, global: &[String]) -> Vec<String> {
        self.settings
            .as_ref()
            .and_then(|s| s.jvm_args.clone())
            .unwrap_or_else(|| global.to_vec())
    }

    pub fn aikar_flags(&self, global: bool) -> bool {
        self.settings
            .as_ref()
            .and_then(|s| s.aikar_flags)
            .unwrap_or(global)
    }
}

fn instance_json_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join("instance.json")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Filesystem-safe form of a version id.
fn sanitise(version_id: &str) -> String {
    version_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect()
}

/// Millisecond timestamp plus a process-wide counter.
///
/// Second resolution alone collides when two instances of the same version are
/// created within the same second: both get the same directory name and the
/// second one silently overwrites the first.
fn unique_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{millis}_{seq}")
}

/// Generates a unique ID: `<version_id_sanitised>_<millis>_<seq>`.
fn new_id(version_id: &str) -> String {
    format!("{}_{}", sanitise(version_id), unique_suffix())
}

/// Generates a unique ID for a copy.
pub fn new_id_for_copy(version_id: &str) -> String {
    format!("{}_copy_{}", sanitise(version_id), unique_suffix())
}

// ─── Public API ────────────────────────────────────────────────

/// Loads all instances from the instances directory. Directories without a
/// valid `instance.json` are silently skipped.
pub fn load_all(instances_root: &Path) -> Result<Vec<Instance>> {
    if !instances_root.exists() {
        return Ok(Vec::new());
    }
    let mut instances: Vec<Instance> = Vec::new();
    for entry in std::fs::read_dir(instances_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let json_path = instance_json_path(&entry.path());
        if !json_path.exists() {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(&json_path) {
            if let Ok(inst) = serde_json::from_str::<Instance>(&raw) {
                instances.push(inst);
            }
        }
    }
    // Sort by creation time, newest first.
    instances.sort_by_key(|i| Reverse(i.created_at));
    Ok(instances)
}

/// Creates a new instance directory and writes `instance.json`.
///
/// The instance starts out as *not installed*: the caller flips the flag with
/// [`mark_installed`] once every file has been downloaded, so an interrupted
/// install never looks like a ready-to-play instance.
pub fn create(
    instances_root: &Path,
    name: String,
    version_id: String,
    loader: Option<String>,
    loader_version: Option<String>,
) -> Result<Instance> {
    let id = new_id(&version_id);
    let instance = Instance {
        id: id.clone(),
        name,
        version_id,
        loader,
        loader_version,
        minecraft_version: None,
        created_at: now_secs(),
        last_played: None,
        installed: Some(false),
        settings: None,
        total_playtime_secs: None,
        modpack_source: None,
    };

    let inst_dir = instance.dir(instances_root);
    std::fs::create_dir_all(inst_dir.join("game"))?;
    std::fs::create_dir_all(inst_dir.join("natives"))?;

    save(instances_root, &instance)?;
    Ok(instance)
}

/// Persists an instance's metadata atomically.
pub fn save(instances_root: &Path, instance: &Instance) -> Result<()> {
    let path = instance_json_path(&instance.dir(instances_root));
    let json = serde_json::to_vec_pretty(instance)?;
    write_atomic(&path, &json)
}

/// Loads a single instance by ID.
pub fn load(instances_root: &Path, id: &str) -> Result<Instance> {
    if id.is_empty() || id.contains("..") || id.contains('/') || id.contains('\\') {
        return Err(NimbusError::Invalid(format!(
            "invalid instance id: '{id}'"
        )));
    }
    let inst_dir = instances_root.join(id);
    let json_path = instance_json_path(&inst_dir);
    let raw = std::fs::read_to_string(&json_path).map_err(|_| {
        NimbusError::Invalid(format!("instance '{id}' not found"))
    })?;
    Ok(serde_json::from_str(&raw)?)
}

/// Marks an instance as fully installed (or back to incomplete).
pub fn mark_installed(instances_root: &Path, id: &str, installed: bool) -> Result<Instance> {
    let mut inst = load(instances_root, id)?;
    inst.installed = Some(installed);
    save(instances_root, &inst)?;
    Ok(inst)
}

/// Links (or clears, with `None`) which Modrinth project/version this
/// instance's modpack content was installed from, enabling
/// `check_modpack_update`/`update_modpack`.
pub fn set_modpack_source(
    instances_root: &Path,
    id: &str,
    source: Option<ModpackSource>,
) -> Result<Instance> {
    let mut inst = load(instances_root, id)?;
    inst.modpack_source = source;
    save(instances_root, &inst)?;
    Ok(inst)
}

/// Replaces the per-instance launch settings.
pub fn set_settings(
    instances_root: &Path,
    id: &str,
    settings: Option<InstanceSettings>,
) -> Result<Instance> {
    let mut inst = load(instances_root, id)?;
    inst.settings = settings;
    save(instances_root, &inst)?;
    Ok(inst)
}

/// Updates the `last_played` timestamp.
pub fn touch_last_played(instances_root: &Path, id: &str) -> Result<()> {
    let mut inst = load(instances_root, id)?;
    inst.last_played = Some(now_secs());
    save(instances_root, &inst)
}

/// Adds a finished session to the instance's play-time counter.
///
/// Called from the process watcher, so a failure here must never surface to the
/// user: losing a few seconds of statistics is preferable to an error toast
/// after a normal game exit.
pub fn add_playtime(instances_root: &Path, id: &str, seconds: u64) -> Result<()> {
    if seconds == 0 {
        return Ok(());
    }
    let mut inst = load(instances_root, id)?;
    inst.total_playtime_secs = Some(inst.total_playtime_secs.unwrap_or(0) + seconds);
    save(instances_root, &inst)
}

/// Total size on disk of the instance directory, in bytes.
pub fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_dir() {
            total += dir_size(&entry.path());
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_id_is_safe_for_filesystem() {
        let id = new_id("1.20.1");
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')));
    }

    #[test]
    fn ids_created_in_the_same_second_do_not_collide() {
        assert_ne!(new_id("1.20.1"), new_id("1.20.1"));
        assert_ne!(new_id_for_copy("1.20.1"), new_id_for_copy("1.20.1"));
    }

    #[test]
    fn create_and_load_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("nimbus_test_{}", unique_suffix()));
        std::fs::create_dir_all(&tmp).unwrap();

        let inst = create(
            &tmp,
            "My Instance".to_owned(),
            "1.20.1".to_owned(),
            None,
            None,
        )
        .unwrap();

        let loaded = load(&tmp, &inst.id).unwrap();
        assert_eq!(loaded.name, "My Instance");
        assert_eq!(loaded.version_id, "1.20.1");
        assert!(loaded.loader.is_none());
        // A freshly created instance is not usable until the install finishes.
        assert!(!loaded.is_installed());

        let done = mark_installed(&tmp, &inst.id, true).unwrap();
        assert!(done.is_installed());

        // Cleanup.
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn legacy_instances_without_the_flag_are_installed() {
        let raw = r#"{
            "id": "a", "name": "n", "versionId": "1.21",
            "loader": null, "loaderVersion": null, "minecraftVersion": null,
            "createdAt": 1, "lastPlayed": null
        }"#;
        let inst: Instance = serde_json::from_str(raw).unwrap();
        assert!(inst.is_installed());
        assert!(inst.settings.is_none());
        assert!(inst.modpack_source.is_none());
    }

    #[test]
    fn modpack_source_roundtrips_via_set_modpack_source() {
        let tmp = std::env::temp_dir().join(format!("nimbus_test_{}", unique_suffix()));
        std::fs::create_dir_all(&tmp).unwrap();

        let inst = create(&tmp, "Pack".to_owned(), "1.21".to_owned(), None, None).unwrap();
        assert!(inst.modpack_source.is_none());

        let linked = set_modpack_source(
            &tmp,
            &inst.id,
            Some(ModpackSource {
                project_id: "abc123".to_owned(),
                version_id: "def456".to_owned(),
            }),
        )
        .unwrap();
        assert_eq!(linked.modpack_source.as_ref().unwrap().project_id, "abc123");

        let reloaded = load(&tmp, &inst.id).unwrap();
        assert_eq!(reloaded.modpack_source.unwrap().version_id, "def456");

        let cleared = set_modpack_source(&tmp, &inst.id, None).unwrap();
        assert!(cleared.modpack_source.is_none());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn per_instance_settings_override_globals() {
        let mut inst: Instance = serde_json::from_str(
            r#"{"id":"a","name":"n","versionId":"1.21","createdAt":1}"#,
        )
        .unwrap();
        assert_eq!(inst.memory_mib(2048), 2048);
        inst.settings = Some(InstanceSettings {
            memory_mib: Some(8192),
            jvm_args: None,
            aikar_flags: Some(true),
        });
        assert_eq!(inst.memory_mib(2048), 8192);
        assert!(inst.aikar_flags(false));
        assert_eq!(inst.jvm_args(&["-Xss1M".to_owned()]), vec!["-Xss1M".to_owned()]);
    }
}
