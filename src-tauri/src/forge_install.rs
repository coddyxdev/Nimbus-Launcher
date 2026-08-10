//! Forge / NeoForge installer processors.
//!
//! Forge 1.13+ does NOT ship a ready-to-run patched client. The installer
//! contains `install_profile.json` describing a chain of "processors" (small
//! Java tools) that generate, on the user's machine:
//!
//! - `net/minecraft/client/<mc>-<ts>/client-<...>-extra.jar` (resources)
//! - `net/minecraft/client/<mc>-<ts>/client-<...>-srg.jar`   (remapped classes)
//! - `net/minecraftforge/forge/<ver>/forge-<ver>-client.jar` (patched client)
//!
//! Without them FML fails with
//! `Could not find net/minecraft/client/Minecraft.class in classloader`,
//! because the classpath only holds the vanilla obfuscated jar.
//!
//! This module replays that pipeline: it extracts the embedded data files and
//! bundled maven artifacts, downloads the processor libraries, resolves the
//! `{TOKEN}` / `[maven:coords]` argument syntax and runs each processor with
//! the local JVM. The whole step is idempotent and marked as done on disk.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::download::{download_one, DownloadTask, ProgressEvent};
use crate::error::{NimbusError, Result};
use crate::{java, libraries, paths};

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";

// ─── install_profile.json ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct InstallProfile {
    #[serde(default)]
    data: HashMap<String, SideValue>,
    #[serde(default)]
    processors: Vec<Processor>,
    #[serde(default)]
    libraries: Vec<ProfileLibrary>,
}

#[derive(Debug, Deserialize)]
struct SideValue {
    #[serde(default)]
    client: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Processor {
    #[serde(default)]
    sides: Vec<String>,
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProfileLibrary {
    name: String,
    #[serde(default)]
    downloads: Option<LibDownloads>,
}

#[derive(Debug, Deserialize)]
struct LibDownloads {
    #[serde(default)]
    artifact: Option<LibArtifact>,
}

#[derive(Debug, Deserialize)]
struct LibArtifact {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

// ─── Helpers ─────────────────────────────────────────────────────

/// Downloads a file without surfacing progress to the UI.
async fn download_quiet(url: String, dest: PathBuf) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ProgressEvent>();
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    download_one(
        DownloadTask {
            url,
            dest,
            hash: None,
            size: None,
        },
        tx,
    )
    .await?;
    Ok(())
}

fn open_installer(installer_jar: &Path) -> Result<zip::ZipArchive<std::fs::File>> {
    let file = std::fs::File::open(installer_jar)?;
    zip::ZipArchive::new(file).map_err(|e| NimbusError::Zip(e.to_string()))
}

/// Reads `install_profile.json` from the installer. Returns `None` for old
/// installers that do not have one (nothing to process).
fn read_install_profile(installer_jar: &Path) -> Result<Option<InstallProfile>> {
    let mut archive = open_installer(installer_jar)?;
    let mut raw = String::new();
    {
        let mut entry = match archive.by_name("install_profile.json") {
            Ok(e) => e,
            Err(_) => return Ok(None),
        };
        entry.read_to_string(&mut raw)?;
    }
    let profile: InstallProfile = serde_json::from_str(&raw)?;
    Ok(Some(profile))
}

/// Copies every `maven/...` entry bundled inside the installer into the shared
/// libraries folder (this is where the Forge universal jar comes from).
fn extract_bundled_maven(installer_jar: &Path, libraries_root: &Path) -> Result<()> {
    let mut archive = open_installer(installer_jar)?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| NimbusError::Zip(e.to_string()))?;
        let name = entry.name().to_owned();
        let rel = match name.strip_prefix("maven/") {
            Some(r) if !r.is_empty() && !r.ends_with('/') => r.to_owned(),
            _ => continue,
        };
        let dest = libraries_root.join(&rel);
        if dest.exists() {
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

/// Extracts one installer entry (`/data/client.lzma` style path) to `data_dir`.
fn extract_data_entry(installer_jar: &Path, entry_path: &str, data_dir: &Path) -> Result<PathBuf> {
    let inner = entry_path.trim_start_matches('/');
    let dest = data_dir.join(inner.replace('/', "_"));
    if dest.exists() {
        return Ok(dest);
    }
    let mut archive = open_installer(installer_jar)?;
    let mut entry = archive
        .by_name(inner)
        .map_err(|_| NimbusError::Invalid(format!("installer has no entry {entry_path}")))?;
    std::fs::create_dir_all(data_dir)?;
    let mut out = std::fs::File::create(&dest)?;
    std::io::copy(&mut entry, &mut out)?;
    Ok(dest)
}

/// Resolves the `Main-Class` attribute of a jar manifest.
fn jar_main_class(jar: &Path) -> Result<String> {
    let file = std::fs::File::open(jar)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| NimbusError::Zip(e.to_string()))?;
    let mut manifest = String::new();
    {
        let mut entry = archive
            .by_name("META-INF/MANIFEST.MF")
            .map_err(|_| NimbusError::Invalid(format!("{} has no manifest", jar.display())))?;
        entry.read_to_string(&mut manifest)?;
    }
    for line in manifest.lines() {
        if let Some(value) = line.strip_prefix("Main-Class:") {
            return Ok(value.trim().to_owned());
        }
    }
    Err(NimbusError::Invalid(format!(
        "{} has no Main-Class",
        jar.display()
    )))
}

/// `[group:artifact:version:classifier@ext]` → absolute path in the library dir.
fn maven_ref_to_path(value: &str, libraries_root: &Path) -> Option<PathBuf> {
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    Some(libraries_root.join(libraries::maven_path(inner)))
}

/// Appends a processor transcript to the install log (best effort).
fn append_log(log_path: &Path, text: &str) {
    use std::io::Write as _;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = file.write_all(text.as_bytes());
    }
}

fn path_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

// ─── Public API ─────────────────────────────────────────────────

/// Runs the Forge/NeoForge installer processors if they have not run yet.
///
/// No-op for vanilla, Fabric and Quilt instances, and for Forge versions whose
/// installer has no `install_profile.json`.
pub async fn ensure_processed(
    loader: Option<&str>,
    loader_version: Option<&str>,
    mc_version: &str,
    libraries_root: &Path,
    client_jar: &Path,
    progress: Option<(&tauri::AppHandle, &str)>,
) -> Result<()> {
    let (loader, loader_version) = match (loader, loader_version) {
        (Some(l), Some(v)) if l == "forge" || l == "neoforge" => (l, v),
        _ => return Ok(()),
    };

    let installers_dir = paths::shared_dir()?.join("installers");
    std::fs::create_dir_all(&installers_dir)?;

    let full_version = if loader == "forge" {
        format!("{mc_version}-{loader_version}")
    } else {
        loader_version.to_owned()
    };
    let marker = installers_dir.join(format!(".processed-{loader}-{full_version}"));
    if marker.exists() {
        return Ok(());
    }

    let installer_jar = installers_dir.join(format!("{loader}-{full_version}-installer.jar"));
    if !installer_jar.exists() {
        let url = if loader == "forge" {
            format!(
                "{FORGE_MAVEN}/{v}/forge-{v}-installer.jar",
                v = full_version
            )
        } else {
            format!(
                "{NEOFORGE_MAVEN}/{v}/neoforge-{v}-installer.jar",
                v = full_version
            )
        };
        download_quiet(url, installer_jar.clone()).await?;
    }

    let profile = match read_install_profile(&installer_jar)? {
        Some(p) => p,
        None => {
            let _ = std::fs::write(&marker, b"no install_profile.json");
            return Ok(());
        }
    };
    if profile.processors.is_empty() {
        let _ = std::fs::write(&marker, b"no processors");
        return Ok(());
    }

    // 1. Artifacts shipped inside the installer (Forge universal jar, ...).
    extract_bundled_maven(&installer_jar, libraries_root)?;

    // 2. Processor libraries.
    for lib in &profile.libraries {
        let artifact = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
        let rel = artifact
            .and_then(|a| a.path.clone())
            .unwrap_or_else(|| libraries::maven_path(&lib.name));
        let dest = libraries_root.join(&rel);
        if dest.exists() {
            continue;
        }
        let url = match artifact.and_then(|a| a.url.clone()) {
            Some(u) if !u.is_empty() => u,
            // Empty URL means "provided by the installer" — already extracted.
            _ => continue,
        };
        download_quiet(url, dest).await?;
    }

    // 3. Token table for processor arguments.
    let data_dir = installers_dir.join(format!("{loader}-{full_version}-data"));
    let mut tokens: HashMap<String, String> = HashMap::new();
    tokens.insert("SIDE".to_owned(), "client".to_owned());
    tokens.insert("MINECRAFT_JAR".to_owned(), path_string(client_jar));
    tokens.insert("MINECRAFT_VERSION".to_owned(), mc_version.to_owned());
    tokens.insert("ROOT".to_owned(), path_string(libraries_root));
    tokens.insert("LIBRARY_DIR".to_owned(), path_string(libraries_root));
    tokens.insert("INSTALLER".to_owned(), path_string(&installer_jar));

    for (key, value) in &profile.data {
        let raw = match value.client.as_deref() {
            Some(v) => v,
            None => continue,
        };
        let resolved = if raw.starts_with('[') {
            maven_ref_to_path(raw, libraries_root)
                .map(|p| path_string(&p))
                .unwrap_or_else(|| raw.to_owned())
        } else if raw.starts_with('/') {
            path_string(&extract_data_entry(&installer_jar, raw, &data_dir)?)
        } else {
            raw.trim_matches('\'').to_owned()
        };
        tokens.insert(key.clone(), resolved);
    }

    // 4. Run the processors.
    let runtimes_dir = paths::runtimes_dir()?;
    let java_bin = java::resolve_java(21, &runtimes_dir).await?;
    let java_exe = if java_bin.is_dir() {
        java_bin
            .join("bin")
            .join(if cfg!(windows) { "java.exe" } else { "java" })
    } else {
        java_bin
    };

    let log_path = installers_dir.join(format!("{loader}-{full_version}-processors.log"));
    let _ = std::fs::remove_file(&log_path);

    // Processors run once per install and can take tens of seconds each, so
    // the UI is told how far along we are.
    let client_processors = profile
        .processors
        .iter()
        .filter(|p| p.sides.is_empty() || p.sides.iter().any(|s| s == "client"))
        .count() as u64;
    let mut processed: u64 = 0;

    for processor in &profile.processors {
        if !processor.sides.is_empty() && !processor.sides.iter().any(|s| s == "client") {
            continue;
        }
        let jar_path = libraries_root.join(libraries::maven_path(&processor.jar));
        if !jar_path.exists() {
            return Err(NimbusError::Invalid(format!(
                "Обработчик Forge не найден: {}",
                jar_path.display()
            )));
        }
        let main_class = jar_main_class(&jar_path)?;

        let mut cp: Vec<String> = vec![path_string(&jar_path)];
        for entry in &processor.classpath {
            cp.push(path_string(
                &libraries_root.join(libraries::maven_path(entry)),
            ));
        }

        let mut args: Vec<String> = Vec::with_capacity(processor.args.len());
        for arg in &processor.args {
            args.push(resolve_arg(arg, &tokens, libraries_root));
        }

        let separator = if cfg!(windows) { ";" } else { ":" };

        // Several processors refuse to create their own output folders.
        for arg in &args {
            let candidate = Path::new(arg);
            if candidate.is_absolute() && candidate.extension().is_some() {
                if let Some(parent) = candidate.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
        }

        let output = tokio::process::Command::new(&java_exe)
            .arg("-cp")
            .arg(cp.join(separator))
            .arg(&main_class)
            .args(&args)
            .output()
            .await?;

        processed += 1;
        if let Some((app, instance_id)) = progress {
            use tauri::Emitter as _;
            let _ = app.emit(
                "launch:stage",
                serde_json::json!({
                    "instanceId": instance_id,
                    "stage": "forge-processors",
                    "done": processed,
                    "total": client_processors,
                }),
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        append_log(
            &log_path,
            &format!(
                "### {main_class}\n{}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}\n",
                args.join(" ")
            ),
        );

        if !output.status.success() {
            let mut lines: Vec<&str> = stderr
                .lines()
                .chain(stdout.lines())
                .filter(|l| !l.trim().is_empty())
                .collect();
            let skip = lines.len().saturating_sub(15);
            lines = lines.split_off(skip);
            return Err(NimbusError::Invalid(format!(
                "Обработчик Forge {main_class} завершился с ошибкой ({}).\nАргументы: {}\nЛог: {}\n{}",
                output.status.code().unwrap_or(-1),
                args.join(" "),
                log_path.display(),
                lines.join("\n")
            )));
        }
    }

    std::fs::write(&marker, b"ok")?;
    Ok(())
}

/// Expands `{TOKEN}` and `[maven:coords]` inside a single processor argument.
fn resolve_arg(arg: &str, tokens: &HashMap<String, String>, libraries_root: &Path) -> String {
    if let Some(path) = maven_ref_to_path(arg, libraries_root) {
        return path_string(&path);
    }
    if let Some(inner) = arg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        if let Some(value) = tokens.get(inner) {
            return value.clone();
        }
    }
    let mut out = arg.to_owned();
    for (key, value) in tokens {
        let needle = format!("{{{key}}}");
        if out.contains(&needle) {
            out = out.replace(&needle, value);
        }
    }
    out.trim_matches('\'').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_refs_are_expanded() {
        let root = Path::new("C:/libs");
        let out = resolve_arg(
            "[net.minecraftforge:forge:1.20.1-47.1.30:client]",
            &HashMap::new(),
            root,
        );
        assert!(out.contains("net"));
        assert!(out.contains("forge"));
    }

    #[test]
    fn tokens_are_expanded() {
        let mut tokens = HashMap::new();
        tokens.insert("SIDE".to_owned(), "client".to_owned());
        assert_eq!(resolve_arg("{SIDE}", &tokens, Path::new("/l")), "client");
        assert_eq!(
            resolve_arg("--side={SIDE}", &tokens, Path::new("/l")),
            "--side=client"
        );
    }

    #[test]
    fn literals_are_unquoted() {
        let out = resolve_arg("'literal'", &HashMap::new(), Path::new("/l"));
        assert_eq!(out, "literal");
    }
}
