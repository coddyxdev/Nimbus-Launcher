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

/// The user''s custom launcher background lives here. Keeping it inside the
/// profile means the asset-protocol scope stays limited to one directory we
/// own, and the picture survives the original file being moved or deleted.
pub fn backgrounds_dir() -> Result<PathBuf> {
    Ok(root()?.join("backgrounds"))
}

/// Creates every directory the launcher relies on. Idempotent.
pub fn ensure_all() -> Result<()> {
    for dir in [
        root()?,
        instances_dir()?,
        shared_dir()?,
        runtimes_dir()?,
        logs_dir()?,
        backgrounds_dir()?,
    ] {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(())
}

/// Joins `base` with a `/`- or `\`-separated relative path taken from an
/// untrusted source (a zip entry name, an asset name, etc.), rejecting any
/// component that would let the result escape `base`: parent-dir segments,
/// absolute paths, and Windows drive letters.
///
/// Shared by every place that unpacks or copies files named by external data
/// (`natives.rs`, `assets.rs`, `commands/backup.rs`, `commands/modpack.rs`)
/// so the traversal check only has to be gotten right in one place.
pub fn safe_join(base: &std::path::Path, relative: &str) -> Option<PathBuf> {
    use std::path::Component;

    if relative.is_empty() {
        return None;
    }

    let rel = relative.replace('\\', "/");
    let rel_path = std::path::Path::new(&rel);

    for component in rel_path.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            _ => {}
        }
    }

    let dest = base.join(rel_path);
    if dest.starts_with(base) {
        Some(dest)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn safe_join_allows_normal_paths() {
        let base = Path::new("/data");
        assert!(safe_join(base, "file.txt").is_some());
        assert!(safe_join(base, "sub/dir/file.txt").is_some());
    }

    #[test]
    fn safe_join_blocks_traversal_and_absolute_paths() {
        let base = Path::new("/data");
        assert!(safe_join(base, "../escape.txt").is_none());
        assert!(safe_join(base, "sub/../../escape.txt").is_none());
        assert!(safe_join(base, "/etc/passwd").is_none());
        assert!(safe_join(base, "").is_none());
    }

    #[test]
    fn safe_join_blocks_drive_letters() {
        let base = Path::new("C:/data");
        assert!(safe_join(base, "C:/other/path").is_none());
    }

    #[test]
    fn safe_join_normalises_backslashes() {
        let base = Path::new("/data");
        assert!(safe_join(base, "sub\\dir\\file.txt").is_some());
    }
}
