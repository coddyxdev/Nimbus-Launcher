//! Native library extraction.
//!
//! Extracts `.jar` files (which are zips) into `<instance>/natives/`.
//! Guards against path traversal in zip entry names. Respects the
//! `extract.exclude` list (e.g. `META-INF/`).
//!
//! Two jar layouts exist in the wild:
//! - Flat (LWJGL 3.3.x and older, jtracy): the shared library sits at the root
//!   of the jar, e.g. `lwjgl.dll`.
//! - Nested (LWJGL 3.4.x, shipped with Minecraft 1.21.9 / 26.x and newer): the
//!   library sits under a platform/arch prefix, e.g.
//!   `windows/x64/org/lwjgl/lwjgl.dll`, and a single jar may carry several
//!   architectures.
//!
//! The JVM does not search `-Djava.library.path` recursively, so every
//! extracted library is flattened to its bare file name. Entries belonging to
//! another operating system or architecture are skipped: flattening them would
//! let the arm64 or x86 copy overwrite the x64 one and the game would die with
//! `UnsatisfiedLinkError: Failed to locate library: lwjgl.dll`.

use std::io::Read;
use std::path::Path;

use crate::error::{NimbusError, Result};
use crate::libraries::ResolvedLib;
use crate::paths::safe_join;

/// Path segments that name an operating system inside a natives jar.
const PLATFORM_SEGMENTS: &[&str] = &[
    "windows", "linux", "macos", "osx", "freebsd", "android", "ios",
];

/// Path segments that name a CPU architecture inside a natives jar.
const ARCH_SEGMENTS: &[&str] = &[
    "x64", "x86_64", "amd64", "x86", "i386", "x32", "arm64", "aarch64", "arm32", "armhf", "arm",
    "ppc64le", "riscv64", "s390x", "mips64",
];

fn current_platform_segments() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["windows"]
    } else if cfg!(target_os = "macos") {
        &["macos", "osx"]
    } else {
        &["linux"]
    }
}

fn current_arch_segments() -> &'static [&'static str] {
    if cfg!(target_arch = "x86_64") {
        &["x64", "x86_64", "amd64"]
    } else if cfg!(target_arch = "aarch64") {
        &["arm64", "aarch64"]
    } else if cfg!(target_arch = "x86") {
        &["x86", "i386", "x32"]
    } else {
        &[]
    }
}

/// File extensions of loadable native libraries on the running platform.
fn native_extensions() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["dll"]
    } else if cfg!(target_os = "macos") {
        &["dylib", "jnilib"]
    } else {
        &["so"]
    }
}

fn has_native_extension(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    if native_extensions()
        .iter()
        .any(|ext| lower.ends_with(&format!(".{ext}")))
    {
        return true;
    }
    // Versioned ELF sonames: libfoo.so.1
    cfg!(target_os = "linux") && lower.contains(".so.")
}

/// Decides where a zip entry from a natives jar should land inside the natives
/// directory.
///
/// Returns `None` for metadata, for entries belonging to another platform or
/// architecture, and for anything that is not a loadable library. Returns the
/// flattened file name otherwise.
pub fn native_entry_target(raw_name: &str) -> Option<String> {
    let normalised = raw_name.replace('\\', "/");
    if normalised.to_ascii_lowercase().starts_with("meta-inf/") {
        return None;
    }

    let mut segments: Vec<&str> = normalised.split('/').filter(|s| !s.is_empty()).collect();
    let file_name = segments.pop()?;
    if file_name == "." || file_name == ".." {
        return None;
    }
    if !has_native_extension(file_name) {
        return None;
    }

    for segment in segments {
        let seg = segment.to_ascii_lowercase();
        let seg = seg.as_str();
        if PLATFORM_SEGMENTS.contains(&seg) && !current_platform_segments().contains(&seg) {
            return None;
        }
        if ARCH_SEGMENTS.contains(&seg) && !current_arch_segments().contains(&seg) {
            return None;
        }
    }

    Some(file_name.to_owned())
}

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
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| NimbusError::Zip(e.to_string()))?;

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
            let normalised = raw_name.replace('\\', "/");
            if lib
                .extract_exclude
                .iter()
                .any(|excl| normalised.starts_with(excl.as_str()))
            {
                continue;
            }

            // Flatten to a bare file name, dropping foreign platforms and
            // architectures as well as jar metadata.
            let Some(target) = native_entry_target(&raw_name) else {
                continue;
            };

            // Path traversal guard: reject any entry that would resolve
            // outside `natives_dir`.
            let dest = match safe_join(natives_dir, &target) {
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

    #[test]
    fn metadata_entries_are_skipped() {
        assert!(native_entry_target("META-INF/MANIFEST.MF").is_none());
        assert!(native_entry_target("META-INF/windows/x64/org/lwjgl/lwjgl.dll.sha1").is_none());
        assert!(native_entry_target("Tracy_LICENSE").is_none());
    }

    #[test]
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    fn nested_lwjgl_entry_is_flattened() {
        assert_eq!(
            native_entry_target("windows/x64/org/lwjgl/lwjgl.dll").as_deref(),
            Some("lwjgl.dll")
        );
        assert_eq!(
            native_entry_target("windows/x64/org/lwjgl/glfw/glfw.dll").as_deref(),
            Some("glfw.dll")
        );
    }

    #[test]
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    fn foreign_platform_and_arch_entries_are_skipped() {
        assert!(native_entry_target("windows/arm64/org/lwjgl/lwjgl.dll").is_none());
        assert!(native_entry_target("windows/x86/org/lwjgl/lwjgl.dll").is_none());
        assert!(native_entry_target("linux/x64/org/lwjgl/liblwjgl.so").is_none());
        assert!(native_entry_target("macos/arm64/org/lwjgl/liblwjgl.dylib").is_none());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn flat_entry_keeps_its_name() {
        assert_eq!(
            native_entry_target("jtracy-jni-windows.dll").as_deref(),
            Some("jtracy-jni-windows.dll")
        );
    }
}
