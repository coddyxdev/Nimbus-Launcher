//! Native library extraction.
//!
//! Extracts `.jar` files (which are zips) into `<instance>/natives/`.
//! Guards against path traversal in zip entry names. Respects the
//! `extract.exclude` list (e.g. `META-INF/`).

use std::io::Read;
use std::path::Path;

use crate::error::{NimbusError, Result};
use crate::libraries::ResolvedLib;

/// Extracts all native libraries for the given resolved list into `natives_dir`.
///
/// The `libraries_root` is where the `.jar` files live after download.
/// Only entries with `is_native = true` are processed.
pub fn extract_natives(
    libs: &[ResolvedLib],
    libraries_root: &Path,
    natives_dir: &Path,
) -> Result<()> {
    std::fs::create_dir_all(natives_dir)?;

    for lib in libs.iter().filter(|l| l.is_native) {
        let jar_path = libraries_root.join(&lib.rel_path);
        if !jar_path.exists() {
            // Should have been downloaded before this is called; skip
            // gracefully so one missing optional native doesn't abort launch.
            continue;
        }

        let file = std::fs::File::open(&jar_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| NimbusError::Zip(e.to_string()))?;

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| NimbusError::Zip(e.to_string()))?;

            let raw_name = entry.name().to_owned();

            // Skip directories.
            if raw_name.ends_with('/') || raw_name.ends_with('\\') {
                continue;
            }

            // Honour extract.exclude list.
            if lib
                .extract_exclude
                .iter()
                .any(|excl| raw_name.starts_with(excl.as_str()))
            {
                continue;
            }

            // Path traversal guard: reject any entry whose components contain
            // `..` or that would resolve outside `natives_dir`.
            let dest = match safe_join(natives_dir, &raw_name) {
                Some(p) => p,
                None => {
                    return Err(NimbusError::Zip(format!(
                        "path traversal detected in native zip entry: {raw_name}"
                    )));
                }
            };

            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let mut out = std::fs::File::create(&dest)?;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = entry.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                std::io::Write::write_all(&mut out, &buf[..n])?;
            }
        }
    }
    Ok(())
}

/// Joins `base` with `relative`, returning `None` if the result would
/// escape `base` (path traversal) or if any component is `..`.
fn safe_join(base: &Path, relative: &str) -> Option<std::path::PathBuf> {
    use std::path::Component;

    // Normalise separators.
    let rel = relative.replace('\\', "/");
    let rel_path = std::path::Path::new(&rel);

    for component in rel_path.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            _ => {}
        }
    }

    let dest = base.join(rel_path);
    // Final canonical check: dest must start with base.
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
        let base = Path::new("/natives");
        assert!(safe_join(base, "lwjgl.dll").is_some());
        assert!(safe_join(base, "subdir/openal.dll").is_some());
    }

    #[test]
    fn safe_join_blocks_traversal() {
        let base = Path::new("/natives");
        assert!(safe_join(base, "../etc/passwd").is_none());
        assert!(safe_join(base, "../../secret").is_none());
    }

    #[test]
    fn safe_join_blocks_absolute_paths() {
        let base = Path::new("/natives");
        assert!(safe_join(base, "/etc/passwd").is_none());
    }

    #[test]
    fn safe_join_normalises_backslash() {
        let base = Path::new("/natives");
        assert!(safe_join(base, "subdir\\lwjgl.dll").is_some());
    }
}
