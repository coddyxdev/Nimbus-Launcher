//! Java runtime discovery and automatic download via Adoptium API v3.
//!
//! Search order for an already-installed JVM:
//! 1. `JAVA_HOME` environment variable.
//! 2. `PATH` entries whose parent directory contains a `release` file.
//! 3. Windows Registry: `HKLM\SOFTWARE\JavaSoft\JDK` and the WOW64 node.
//! 4. Standard installation directories: Adoptium, Eclipse, Oracle, Zulu.
//!
//! Major version is read from the `release` file (`JAVA_VERSION` line) or,
//! as a fallback, from `java -version` stderr. The major is extracted from
//! the numeric prefix only — never guessed from a version string pattern.
//!
//! Auto-download uses Adoptium API v3. The JRE is extracted to
//! `runtimes/<major>/` and `javaw.exe` inside is returned.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::download::{client, DownloadTask, ExpectedHash, ProgressEvent, download_one};
use crate::error::{NimbusError, Result};

// ─── Major version from release file ─────────────────────────────────────────

/// Parses `JAVA_VERSION="21.0.3"` or `JAVA_VERSION=1.8.0_362` from the
/// `release` file that ships with every modern JDK/JRE.
fn major_from_release_file(java_home: &Path) -> Option<u32> {
    let release = java_home.join("release");
    let text = std::fs::read_to_string(release).ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("JAVA_VERSION=") {
            let ver = rest.trim_matches('"');
            return parse_major(ver);
        }
    }
    None
}

/// Parses the major version integer from a Java version string:
/// - `21.0.3` → 21
/// - `1.8.0_362` → 8 (the 1.x family: major is the second component)
/// - `17` → 17
pub fn parse_major(ver: &str) -> Option<u32> {
    let first: &str = ver.split(['.', '-', '_', '+']).next()?;
    let n: u32 = first.parse().ok()?;
    if n == 1 {
        // Legacy 1.x scheme: 1.8.0 → major 8.
        let second = ver.split('.').nth(1)?;
        second.parse().ok()
    } else {
        Some(n)
    }
}

/// Reads the major version from `java -version` stderr (fallback when there is
/// no `release` file).
fn major_from_java_bin(java: &Path) -> Option<u32> {
    let output = Command::new(java)
        .arg("-version")
        .output()
        .ok()?;
    // `-version` prints to stderr.
    let text = String::from_utf8_lossy(&output.stderr).into_owned();
    for line in text.lines() {
        // Matches: `java version "21.0.3"` and `openjdk version "17.0.5"`
        if let Some(start) = line.find('"') {
            let rest = &line[start + 1..];
            if let Some(end) = rest.find('"') {
                return parse_major(&rest[..end]);
            }
        }
    }
    None
}

/// Returns the major version of the JVM at `java_home`, or `None` if it
/// cannot be determined.
pub fn java_home_major(java_home: &Path) -> Option<u32> {
    major_from_release_file(java_home)
        .or_else(|| major_from_java_bin(&java_home.join("bin").join("java.exe")))
}

// ─── Discovery ───────────────────────────────────────────────────────────────

fn javaw(home: &Path) -> PathBuf {
    home.join("bin").join("javaw.exe")
}

fn candidate_java_homes() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. JAVA_HOME
    if let Ok(jh) = std::env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(jh));
    }

    // 2. java.exe entries on PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let exe = dir.join("java.exe");
            if exe.is_file() {
                // The bin directory is one level up from the home.
                if let Some(home) = dir.parent() {
                    candidates.push(home.to_path_buf());
                }
            }
        }
    }

    // 3. Windows Registry
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
        use winreg::RegKey;

        for root_path in &[
            r"SOFTWARE\JavaSoft\JDK",
            r"SOFTWARE\WOW6432Node\JavaSoft\JDK",
            r"SOFTWARE\JavaSoft\Java Runtime Environment",
            r"SOFTWARE\WOW6432Node\JavaSoft\Java Runtime Environment",
            r"SOFTWARE\Eclipse Foundation\JDK",
            r"SOFTWARE\Eclipse Adoptium\JRE",
            r"SOFTWARE\Eclipse Adoptium\JDK",
        ] {
            if let Ok(root) = RegKey::predef(HKEY_LOCAL_MACHINE)
                .open_subkey_with_flags(root_path, KEY_READ)
            {
                for subkey_name in root.enum_keys().flatten() {
                    if let Ok(subkey) = root.open_subkey(&subkey_name) {
                        if let Ok(home) = subkey.get_value::<String, _>("JavaHome") {
                            candidates.push(PathBuf::from(home));
                        }
                    }
                }
            }
        }
    }

    // 4. Standard installation directories
    let program_files = std::env::var("PROGRAMFILES")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Program Files"));
    let pf_x86 = std::env::var("PROGRAMFILES(X86)")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Program Files (x86)"));

    let vendors = [
        "Eclipse Adoptium",
        "Eclipse Foundation",
        "Microsoft",
        "Oracle",
        "Zulu",
        "Amazon Corretto",
        "BellSoft",
        "GraalVM",
    ];
    for base in &[&program_files, &pf_x86] {
        for vendor in &vendors {
            let vendor_dir = base.join(vendor);
            if let Ok(entries) = std::fs::read_dir(&vendor_dir) {
                for entry in entries.flatten() {
                    candidates.push(entry.path());
                    // Some layouts: vendor/jdk-21/bin/javaw.exe
                    // Others: vendor/jdk-21.0.3+7/bin/javaw.exe
                }
            }
        }
        // Flat layout: C:\Program Files\Java\jdk-21.0.3
        let java_dir = base.join("Java");
        if let Ok(entries) = std::fs::read_dir(&java_dir) {
            for entry in entries.flatten() {
                candidates.push(entry.path());
            }
        }
    }

    candidates
}

/// Finds the `javaw.exe` for the requested major version among locally
/// installed JVMs. Returns `None` when nothing suitable is found.
pub fn find_local_java(major: u32) -> Option<PathBuf> {
    for home in candidate_java_homes() {
        if !javaw(&home).is_file() {
            continue;
        }
        if let Some(found) = java_home_major(&home) {
            if found == major {
                return Some(javaw(&home));
            }
        }
    }
    None
}

// ─── Adoptium auto-download ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AdoptiumPackage {
    link: String,
    checksum: String,
    name: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Debug, Deserialize)]
struct AdoptiumRelease {
    binary: AdoptiumBinary,
}

/// Downloads a JRE for `major` from the Adoptium API and extracts it to
/// `runtimes_dir/<major>`. Returns the path to `javaw.exe`.
pub async fn download_java(major: u32, runtimes_dir: &Path) -> Result<PathBuf> {
    // The braces used to be literal: in a format! string "{{" produces "{",
    // so every request went to "{https://api.adoptium.net/.../21}/hotspot?..."
    // and automatic Java download could never succeed.
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot\
         ?os=windows&architecture=x64&image_type=jre"
    );
    let resp = client().get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(NimbusError::Http {
            status: resp.status().as_u16(),
            url,
            retriable: false,
        });
    }

    let releases: Vec<AdoptiumRelease> = resp.json().await?;
    let release = releases
        .into_iter()
        .next()
        .ok_or_else(|| NimbusError::JavaNotFound(major))?;

    let pkg = release.binary.package;
    let dest_dir = runtimes_dir.join(major.to_string());
    tokio::fs::create_dir_all(&dest_dir).await?;

    let archive_path = dest_dir.join(&pkg.name);
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<ProgressEvent>();
    // Drain progress events without blocking.
    tokio::spawn(async move {
        while progress_rx.recv().await.is_some() {}
    });

    download_one(
        DownloadTask {
            url: pkg.link,
            dest: archive_path.clone(),
            hash: Some(ExpectedHash::Sha256(pkg.checksum)),
            size: Some(pkg.size),
        },
        progress_tx,
    )
    .await?;

    // Extract the zip.
    extract_jre_zip(&archive_path, &dest_dir)?;
    if let Err(err) = std::fs::remove_file(&archive_path) {
        crate::nlog!("java: failed to remove downloaded archive {archive_path:?} ({err})");
    }

    find_javaw_in_dir(&dest_dir).ok_or(NimbusError::JavaNotFound(major))
}

fn extract_jre_zip(archive: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| NimbusError::Zip(e.to_string()))?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| NimbusError::Zip(e.to_string()))?;
        let name = entry.name().to_owned();

        // Strip the top-level directory that Adoptium bundles always contain.
        let rel = strip_top_dir(&name);
        if rel.is_empty() || rel == "/" {
            continue;
        }

        // Path traversal guard, shared with natives/assets/backup extraction.
        // The old check only looked for "..", but Path::join lets an entry
        // named "C:\Windows\..." (or "/etc/...") replace the destination
        // outright, so a hostile or corrupt archive could write anywhere.
        let Some(dest) = crate::paths::safe_join(dest_dir, &rel) else {
            return Err(NimbusError::Zip(format!(
                "path traversal in JRE archive: {name}"
            )));
        };
        if name.ends_with('/') {
            std::fs::create_dir_all(&dest)?;
            continue;
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut out = std::fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

fn strip_top_dir(path: &str) -> String {
    // `jdk-21.0.3+7-jre/bin/javaw.exe` → `bin/javaw.exe`
    if let Some(idx) = path.find('/') {
        path[idx + 1..].to_owned()
    } else {
        path.to_owned()
    }
}

fn find_javaw_in_dir(dir: &Path) -> Option<PathBuf> {
    // Walk one level of subdirectories looking for bin/javaw.exe
    // (Adoptium extracts as: dest_dir/bin/javaw.exe after strip)
    let direct = dir.join("bin").join("javaw.exe");
    if direct.is_file() {
        return Some(direct);
    }
    // Sometimes there is still one extra nesting level.
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("bin").join("javaw.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Returns the path to `javaw.exe` for `major`, downloading if necessary.
pub async fn resolve_java(major: u32, runtimes_dir: &Path) -> Result<PathBuf> {
    // Check the managed runtimes directory first.
    let managed = runtimes_dir.join(major.to_string());
    if managed.exists() {
        if let Some(jw) = find_javaw_in_dir(&managed) {
            return Ok(jw);
        }
    }

    // Then search the system.
    if let Some(jw) = find_local_java(major) {
        return Ok(jw);
    }

    // Auto-download from Adoptium.
    download_java(major, runtimes_dir).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_major_modern() {
        assert_eq!(parse_major("21.0.3"), Some(21));
        assert_eq!(parse_major("17"), Some(17));
        assert_eq!(parse_major("11.0.21+9"), Some(11));
    }

    #[test]
    fn parse_major_legacy() {
        assert_eq!(parse_major("1.8.0_362"), Some(8));
        assert_eq!(parse_major("1.8.0"), Some(8));
    }

    #[test]
    fn strip_top_dir_works() {
        assert_eq!(
            strip_top_dir("jdk-21.0.3+7-jre/bin/javaw.exe"),
            "bin/javaw.exe"
        );
        assert_eq!(strip_top_dir("nodir"), "nodir");
    }
}
