use std::path::PathBuf;

use crate::error::{NimbusError, Result};

/// Root data directory: %APPDATA%\NimbusClient on Windows.
pub fn root() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or(NimbusError::NoConfigDir)?;
    Ok(base.join("NimbusClient"))
}

pub fn config_file() -> Result<PathBuf> {
    Ok(root()?.join("config.json"))
}

/// Per-instance isolated game directories live here (stage 3).
pub fn instances_dir() -> Result<PathBuf> {
    Ok(root()?.join("instances"))
}

/// Shared, content-addressed assets and libraries (stage 2).
pub fn shared_dir() -> Result<PathBuf> {
    Ok(root()?.join("shared"))
}

/// Downloaded JRE runtimes (stage 2).
pub fn runtimes_dir() -> Result<PathBuf> {
    Ok(root()?.join("runtimes"))
}

pub fn logs_dir() -> Result<PathBuf> {
    Ok(root()?.join("logs"))
}

/// Creates every directory the launcher relies on. Idempotent.
pub fn ensure_all() -> Result<()> {
    for dir in [
        root()?,
        instances_dir()?,
        shared_dir()?,
        runtimes_dir()?,
        logs_dir()?,
    ] {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(())
}
