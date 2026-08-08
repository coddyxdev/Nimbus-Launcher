//! State and helpers shared by the command modules.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::error::{NimbusError, Result};
use crate::libraries;

/// Base Maven repository used when a library entry carries no absolute URL.
pub const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

/// A poisoned mutex means another thread panicked while holding it. The data we
/// keep behind these locks is plain bookkeeping, so recovering is always safe.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A live game process.
///
/// The watcher task owns the `Child` handle, so killing goes through a channel
/// instead of a raw PID. That is what makes termination safe: the child is not
/// reaped until the watcher exits, so its PID cannot be recycled by Windows and
/// handed to an unrelated process in the meantime.
pub struct GameHandle {
    pub pid: u32,
    /// Signals the watcher task to terminate the process tree.
    pub kill: tokio::sync::mpsc::UnboundedSender<()>,
}

/// Tracks running game processes so we can kill them.
pub struct RunningGames {
    pub games: Mutex<HashMap<String, GameHandle>>,
}

impl RunningGames {
    pub fn new() -> Self {
        Self {
            games: Mutex::new(HashMap::new()),
        }
    }

    pub fn is_running(&self, instance_id: &str) -> bool {
        lock(&self.games).contains_key(instance_id)
    }

    /// Asks the watcher task to terminate the game. Returns the PID when a
    /// process was actually registered, `None` when nothing is running.
    pub fn request_kill(&self, instance_id: &str) -> Option<u32> {
        let games = lock(&self.games);
        let handle = games.get(instance_id)?;
        // A closed receiver means the watcher already finished; treat that as
        // "not running" rather than reporting a spurious failure.
        handle.kill.send(()).ok()?;
        Some(handle.pid)
    }
}

impl Default for RunningGames {
    fn default() -> Self {
        Self::new()
    }
}

/// Cooperative cancellation for the current install.
///
/// Downloads are checked between stages and between batches, so cancelling is
/// near-instant in practice without tearing a file mid-write.
#[derive(Default)]
pub struct InstallCancel {
    requested: AtomicBool,
}

impl InstallCancel {
    pub fn request(&self) {
        self.requested.store(true, Ordering::SeqCst);
    }

    pub fn reset(&self) {
        self.requested.store(false, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }

    /// Errors with [`NimbusError::Cancelled`] when the user pressed cancel.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(NimbusError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Payload emitted as `install:progress` during installation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub stage: String,
    pub file: String,
    pub done: u64,
    pub total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl InstallProgress {
    /// A stage marker without byte counters.
    pub fn stage(stage: &str, file: impl Into<String>) -> Self {
        Self {
            stage: stage.to_owned(),
            file: file.into(),
            done: 0,
            total: 1,
            bytes_done: 0,
            bytes_total: 0,
        }
    }
}

/// Builds the download URL for a resolved library. Entries coming from loader
/// profiles usually carry an absolute URL; Mojang's own entries may not, and
/// then the canonical libraries repository is used.
pub fn library_url(lib: &libraries::ResolvedLib) -> String {
    if lib.url.starts_with("http") {
        lib.url.clone()
    } else {
        format!("{MOJANG_LIBRARIES}{}", lib.rel_path)
    }
}

pub fn validate_instance_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(NimbusError::Invalid(
            "Название сборки не может быть пустым".to_owned(),
        ));
    }
    if trimmed.chars().count() > 80 {
        return Err(NimbusError::Invalid(
            "Название сборки слишком длинное (максимум 80 символов)".to_owned(),
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(NimbusError::Invalid(
            "Название сборки содержит недопустимые символы".to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

pub fn validate_username(raw: &str) -> Result<String> {
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() || trimmed.chars().count() > 16 {
        return Err(NimbusError::Invalid(
            "Ник должен содержать от 1 до 16 символов".to_string(),
        ));
    }
    if !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(NimbusError::Invalid(
            "Ник может содержать только латинские буквы, цифры и подчёркивание".to_string(),
        ));
    }
    Ok(trimmed)
}

/// Rejects file names that could escape the intended directory.
pub fn validate_file_name(file_name: &str) -> Result<()> {
    if file_name.is_empty()
        || file_name.contains("..")
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name.contains(':')
    {
        return Err(NimbusError::Invalid(format!(
            "недопустимое имя файла: '{file_name}'"
        )));
    }
    Ok(())
}

/// Rejects instance ids that could escape the instances directory.
///
/// Instance ids are generated by the backend, but every command that accepts
/// one is invoked directly from the WebView, so a compromised or buggy
/// frontend call must not be able to smuggle a path traversal through it
/// (e.g. `delete_instance("../..")`). The rules are identical to file names:
/// ids are plain directory-name strings with no separators.
pub fn validate_instance_id(instance_id: &str) -> Result<()> {
    validate_file_name(instance_id)
}

/// Destructive operations are refused while the game is alive: on Windows a
/// running JVM keeps jars locked, leaving a half-deleted instance behind.
pub fn ensure_not_running(app: &AppHandle, instance_id: &str) -> Result<()> {
    if app.state::<RunningGames>().is_running(instance_id) {
        return Err(NimbusError::Running);
    }
    Ok(())
}

/// Reveals a directory in Explorer, creating it first so the call never fails
/// just because the game has not written anything there yet.
pub async fn reveal_dir(dir: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    let path = dir.to_string_lossy().to_string();
    // tokio's Command keeps the spawn off the async worker thread.
    tokio::process::Command::new("explorer")
        .arg(&path)
        .spawn()
        .map_err(|e| NimbusError::Invalid(format!("не удалось открыть папку: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(url: &str, rel: &str) -> libraries::ResolvedLib {
        libraries::ResolvedLib {
            name: "g:a:1".to_owned(),
            rel_path: rel.to_owned(),
            url: url.to_owned(),
            sha1: String::new(),
            size: 0,
            is_native: false,
            extract_exclude: vec![],
        }
    }

    #[test]
    fn absolute_library_url_is_kept() {
        let l = lib("https://maven.fabricmc.net/a.jar", "a.jar");
        assert_eq!(library_url(&l), "https://maven.fabricmc.net/a.jar");
    }

    #[test]
    fn relative_library_url_gets_mojang_base_without_braces() {
        let l = lib("libraries", "g/a/1/a-1.jar");
        assert_eq!(
            library_url(&l),
            "https://libraries.minecraft.net/g/a/1/a-1.jar"
        );
        assert!(!library_url(&l).contains('{'));
    }

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(validate_instance_name("  My pack ").unwrap(), "My pack");
        assert!(validate_instance_name("   ").is_err());
        assert!(validate_instance_name(&"x".repeat(81)).is_err());
        assert!(validate_instance_name("bad\nname").is_err());
    }

    #[test]
    fn usernames_are_ascii_only() {
        assert_eq!(validate_username(" Amioka ").unwrap(), "Amioka");
        assert!(validate_username("Ник").is_err());
        assert!(validate_username("way_too_long_username").is_err());
    }

    #[test]
    fn traversal_file_names_are_rejected() {
        assert!(validate_file_name("mod.jar").is_ok());
        assert!(validate_file_name("../mod.jar").is_err());
        assert!(validate_file_name("sub/mod.jar").is_err());
        assert!(validate_file_name("C:mod.jar").is_err());
    }

    #[test]
    fn cancel_token_round_trips() {
        let c = InstallCancel::default();
        assert!(c.check().is_ok());
        c.request();
        assert!(c.is_cancelled());
        assert!(matches!(c.check(), Err(NimbusError::Cancelled)));
        c.reset();
        assert!(c.check().is_ok());
    }
}
