use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::error::{NimbusError, Result};
use crate::paths;

/// Bump this whenever the on-disk shape changes, and add a migration arm in
/// `migrate`. Reading a config with a higher version is a hard error rather
/// than a silent downgrade, so we never corrupt a newer profile.
pub const CONFIG_VERSION: u32 = 3;

/// Serialises every read-modify-write cycle on the config file.
///
/// Without it two commands firing at once (theme toggle + nickname change)
/// both read the same snapshot and the last writer silently drops the other
/// field.
fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub version: u32,
    pub theme: Theme,
    /// Default heap size in MiB applied to new instances.
    pub default_memory_mib: u32,
    /// Extra JVM arguments applied to new instances.
    pub default_jvm_args: Vec<String>,
    /// Whether Aikar's GC flags are enabled (v2+).
    pub default_aikar_flags: bool,
    /// Offline nickname used until Microsoft sign-in lands in stage 5.
    pub offline_username: Option<String>,
    /// Azure application id for Microsoft OAuth. Empty until the user
    /// registers one; stage 5 surfaces an explicit error instead of a
    /// placeholder UUID.
    pub azure_client_id: Option<String>,
    /// Whether the two-screen onboarding has been completed.
    pub onboarding_done: bool,
    /// Explicit `javaw.exe` path chosen by the user (v3+). `None` means the
    /// launcher auto-detects a runtime and downloads one when needed.
    #[serde(default)]
    pub java_path: Option<String>,
    /// Initial game window size (v3+). `None` leaves it to Minecraft.
    #[serde(default)]
    pub game_width: Option<u32>,
    #[serde(default)]
    pub game_height: Option<u32>,
    /// Start the game in fullscreen (v3+).
    #[serde(default)]
    pub game_fullscreen: bool,
    /// Publish "playing <instance>" to Discord (v3+). Opt-out, not opt-in,
    /// because it needs no account and no network of its own.
    #[serde(default = "default_true")]
    pub discord_rpc: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            theme: Theme::Dark,
            default_memory_mib: 4096,
            default_jvm_args: vec![],
            default_aikar_flags: true,
            offline_username: None,
            azure_client_id: None,
            onboarding_done: false,
            java_path: None,
            game_width: None,
            game_height: None,
            game_fullscreen: false,
            discord_rpc: true,
        }
    }
}

/// Loads the config, creating a default one if absent.
///
/// A corrupt file is not fatal: it is renamed to `config.corrupt.json` and a
/// fresh default takes its place, so the launcher always starts.
pub fn load() -> Result<Config> {
    let _guard = lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    load_unlocked()
}

/// Same as [`load`], but assumes the caller already holds the config lock.
fn load_unlocked() -> Result<Config> {
    paths::ensure_all()?;
    let path = paths::config_file()?;

    if !path.exists() {
        let cfg = Config::default();
        save_unlocked(&cfg)?;
        return Ok(cfg);
    }

    let raw = fs::read_to_string(&path)?;
    match serde_json::from_str::<Config>(&raw) {
        Ok(cfg) => {
            // The migrated value used to be returned without ever being
            // written back, so an old file was re-migrated on every single
            // load and anything a future migration backfills would be lost
            // again as soon as another writer started from a stale snapshot.
            let found_version = cfg.version;
            let migrated = migrate(cfg)?;
            if migrated.version != found_version {
                save_unlocked(&migrated)?;
            }
            Ok(migrated)
        }
        Err(_) => {
            let quarantine = path.with_file_name("config.corrupt.json");
            if let Err(err) = fs::rename(&path, &quarantine) {
                crate::nlog!("config: failed to quarantine corrupt config.json ({err})");
            }
            let cfg = Config::default();
            save_unlocked(&cfg)?;
            Ok(cfg)
        }
    }
}

fn migrate(mut cfg: Config) -> Result<Config> {
    if cfg.version > CONFIG_VERSION {
        return Err(NimbusError::ConfigTooNew {
            found: cfg.version,
            supported: CONFIG_VERSION,
        });
    }
    // Version 0 -> 1: placeholder; version 1 -> 2: add default_aikar_flags
    if cfg.version == 1 {
        cfg.default_aikar_flags = true;
        cfg.default_jvm_args = vec![];
    }
    // Version 2 -> 3: java_path / game window fields. They are `#[serde(default)]`
    // so an old file already deserialises; nothing to backfill.
    if cfg.version < CONFIG_VERSION {
        cfg.version = CONFIG_VERSION;
    }
    Ok(cfg)
}

/// Read-modify-write under the config lock: the only supported way to change
/// settings. Returns the persisted config.
pub fn update<F>(mutate: F) -> Result<Config>
where
    F: FnOnce(&mut Config) -> Result<()>,
{
    let _guard = lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut cfg = load_unlocked()?;
    mutate(&mut cfg)?;
    save_unlocked(&cfg)?;
    Ok(cfg)
}

/// Atomic write: serialise to `config.json.tmp`, fsync, then rename over the
/// target. A crash mid-write can never leave a half-written config.
#[allow(dead_code)] // kept for callers outside the update() flow
pub fn save(cfg: &Config) -> Result<()> {
    let _guard = lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    save_unlocked(cfg)
}

/// Same as [`save`], but assumes the caller already holds the config lock.
fn save_unlocked(cfg: &Config) -> Result<()> {
    paths::ensure_all()?;
    let path = paths::config_file()?;
    write_atomic(&path, serde_json::to_vec_pretty(cfg)?.as_slice())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    // Appended, not substituted: with_extension() rewrites the existing
    // extension, so "config.json" became "config.tmp" and
    // "game-2026-08-09.log" became "game-2026-08-09.tmp" - two different
    // targets could end up sharing one temp file and racing each other.
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_current_version() {
        assert_eq!(Config::default().version, CONFIG_VERSION);
    }

    #[test]
    fn migrate_rejects_future_versions() {
        let cfg = Config {
            version: CONFIG_VERSION + 1,
            ..Default::default()
        };
        assert!(migrate(cfg).is_err());
    }

    #[test]
    fn migrate_upgrades_older_versions() {
        let cfg = Config {
            version: 0,
            ..Default::default()
        };
        let migrated = migrate(cfg).expect("older config should migrate");
        assert_eq!(migrated.version, CONFIG_VERSION);
    }

    #[test]
    fn config_lock_is_reentrant_free() {
        // Two sequential lock acquisitions must not deadlock: update() and
        // save() never nest, they both call the *_unlocked variants.
        {
            let _a = lock().lock().unwrap();
        }
        let _b = lock().lock().unwrap();
    }
}
