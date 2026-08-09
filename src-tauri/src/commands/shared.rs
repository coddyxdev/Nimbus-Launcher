//! State and helpers shared by the command modules.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

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
    /// Instances whose launch preparation has started but which do not have a
    /// `GameHandle` yet. Preparing a launch is await-heavy (metadata, Java,
    /// Forge processors, native extraction), so without this a second launch
    /// request slipped past the "is it running?" check and started a second
    /// JVM in the same game directory.
    pub starting: Mutex<HashSet<String>>,
}

impl RunningGames {
    pub fn new() -> Self {
        Self {
            games: Mutex::new(HashMap::new()),
            starting: Mutex::new(HashSet::new()),
        }
    }

    /// True when a game is live *or* a launch for it is being prepared.
    ///
    /// The two locks are taken one after another and never nested, so this can
    /// never invert the lock order used by [`LaunchGuard::acquire`].
    pub fn is_running(&self, instance_id: &str) -> bool {
        if lock(&self.starting).contains(instance_id) {
            return true;
        }
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

/// Claims the "launch in progress" slot for one instance and releases it when
/// dropped, including on every early return of the launch command.
///
/// The claim is made under the `starting` lock together with a check of the
/// live-game map, so two concurrent `launch_instance` calls for the same
/// instance can never both proceed. The guard is held until the `GameHandle`
/// has been registered, which leaves no unguarded window in between.
pub struct LaunchGuard {
    app: AppHandle,
    instance_id: String,
}

impl LaunchGuard {
    pub fn acquire(app: &AppHandle, instance_id: &str) -> Result<Self> {
        let state = app.state::<RunningGames>();
        let mut starting = lock(&state.starting);
        if starting.contains(instance_id) || lock(&state.games).contains_key(instance_id) {
            return Err(NimbusError::Running);
        }
        starting.insert(instance_id.to_owned());
        Ok(Self {
            app: app.clone(),
            instance_id: instance_id.to_owned(),
        })
    }
}

impl Drop for LaunchGuard {
    fn drop(&mut self) {
        let state = self.app.state::<RunningGames>();
        lock(&state.starting).remove(&self.instance_id);
    }
}

/// Cooperative cancellation for long-running background operations.
///
/// Keyed per operation rather than one global flag: with a single flag,
/// cancelling one install also aborted an unrelated verify running at the same
/// time, and starting a second operation cleared the flag the first one was
/// still waiting on.
///
/// Downloads are checked between stages and between batches, so cancelling is
/// near-instant in practice without tearing a file mid-write.
#[derive(Default)]
pub struct InstallCancel {
    flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl InstallCancel {
    /// Requests cancellation of one operation. `false` means nothing is
    /// registered under that key: it already finished, or never started.
    pub fn request(&self, key: &str) -> bool {
        match lock(&self.flags).get(key) {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }

    /// Requests cancellation of every registered operation, for the UI's
    /// global cancel button which has no operation key at hand.
    pub fn request_all(&self) {
        for flag in lock(&self.flags).values() {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// Keys of the operations currently registered.
    pub fn active(&self) -> Vec<String> {
        lock(&self.flags).keys().cloned().collect()
    }
}

/// Handle owned by one running operation, which deregisters itself on drop.
pub struct CancelToken {
    app: AppHandle,
    key: String,
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    /// Registers `key` with a fresh flag, replacing any leftover entry.
    pub fn begin(app: &AppHandle, key: &str) -> Self {
        let flag = Arc::new(AtomicBool::new(false));
        lock(&app.state::<InstallCancel>().flags).insert(key.to_owned(), Arc::clone(&flag));
        Self {
            app: app.clone(),
            key: key.to_owned(),
            flag,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
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

impl Drop for CancelToken {
    fn drop(&mut self) {
        // Deregisters on every exit path, including `?` returns. Compared by
        // pointer, so a token that has already been replaced by a newer run of
        // the same operation does not remove that newer entry.
        let state = self.app.state::<InstallCancel>();
        let mut flags = lock(&state.flags);
        if flags
            .get(&self.key)
            .is_some_and(|flag| Arc::ptr_eq(flag, &self.flag))
        {
            flags.remove(&self.key);
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

/// Opens a URL in the user's default browser.
///
/// Only `http`/`https` pass: descriptions, changelogs and login pages all come
/// from the network, and handing an arbitrary scheme to the shell would let
/// remote text start a local program.
pub fn open_external_url(url: &str) -> Result<()> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(NimbusError::Invalid(
            "Ссылку можно открыть только по http или https".to_owned(),
        ));
    }
    // A space or a quote in the argument would let the text past rundll32's own
    // parsing, so anything that could break out of the argument is refused.
    if url.contains(['"', '\'', '\n', '\r', ' ']) {
        return Err(NimbusError::Invalid(
            "Ссылка содержит недопустимые символы".to_owned(),
        ));
    }
    // rundll32 FileProtocolHandler opens the default browser without pulling in
    // a shell plugin.
    std::process::Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map_err(|e| NimbusError::Invalid(format!("Не удалось открыть браузер: {e}")))?;
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

    #[test]
    fn external_url_accepts_only_web_schemes() {
        assert!(open_external_url("file:///C:/Windows/System32/cmd.exe").is_err());
        assert!(open_external_url("javascript:alert(1)").is_err());
        assert!(open_external_url("").is_err());
    }

    #[test]
    fn external_url_rejects_argument_breakouts() {
        assert!(open_external_url("https://example.com/a b").is_err());
        assert!(open_external_url("https://example.com/\"x").is_err());
    }

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
    fn cancel_is_scoped_to_one_operation() {
        // Built by hand instead of through CancelToken, which needs a live
        // AppHandle; the registry logic is what matters here.
        let reg = InstallCancel::default();
        let install = Arc::new(AtomicBool::new(false));
        let verify = Arc::new(AtomicBool::new(false));
        lock(&reg.flags).insert("install:a".to_owned(), Arc::clone(&install));
        lock(&reg.flags).insert("verify:b".to_owned(), Arc::clone(&verify));

        assert!(reg.request("install:a"));
        assert!(install.load(Ordering::SeqCst));
        assert!(
            !verify.load(Ordering::SeqCst),
            "cancelling one operation must not cancel another"
        );
        assert!(!reg.request("nothing:here"));
        assert_eq!(reg.active().len(), 2);

        reg.request_all();
        assert!(verify.load(Ordering::SeqCst));
    }
}
