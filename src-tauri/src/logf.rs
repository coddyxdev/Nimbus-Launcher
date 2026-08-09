//! Best-effort launcher log.
//!
//! The launcher is a windowed (`windows_subsystem = "windows"`) release build,
//! so every `eprintln!` in the codebase went to a stderr nobody can read: the
//! diagnostics that matter most in bug reports (DPAPI falling back to plain
//! text, a corrupt config being quarantined, Rich Presence failing, log
//! pruning failing) were effectively discarded. They now land in a file next
//! to the game logs, which the UI already knows how to open.
//!
//! Logging is never allowed to fail a caller: every error here is swallowed on
//! purpose, because a launcher must not break because it could not write a log
//! line.

use std::fs::OpenOptions;
use std::io::Write;

/// The file is truncated once it grows past this. A rolling scheme would be
/// nicer, but this log is written a few times per session, so a single 2 MiB
/// cap is enough to keep it from growing forever.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Appends one timestamped line to `%APPDATA%\NimbusClient\logs\launcher.log`.
///
/// Prefer the [`crate::nlog!`] macro over calling this directly.
pub fn log_line(message: &str) {
    // Still useful when running `cargo tauri dev` from a terminal.
    if cfg!(debug_assertions) {
        eprintln!("[nimbus] {message}");
    }
    let _ = append(message);
}

/// Returns `None` on any failure; the caller ignores it.
fn append(message: &str) -> Option<()> {
    let dir = crate::paths::logs_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("launcher.log");

    if std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0) > MAX_BYTES {
        let _ = std::fs::remove_file(&path);
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    writeln!(file, "[{stamp}] {message}").ok()?;
    Some(())
}

/// `eprintln!`-shaped diagnostic that actually survives in a release build.
///
/// ```ignore
/// crate::nlog!("config: failed to quarantine corrupt config.json ({err})");
/// ```
#[macro_export]
macro_rules! nlog {
    ($($arg:tt)*) => {
        $crate::logf::log_line(&format!($($arg)*))
    };
}
