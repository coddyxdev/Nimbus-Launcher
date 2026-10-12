//! Discovery and validation of the Nimbus runtime shipped with the launcher.
//!
//! The runtime is a real launch dependency, not an optional decoration: a Nimbus
//! instance must never silently turn into a vanilla launch when its agent is
//! missing or damaged. Packaged files are validated before they replace an
//! installed copy, and the installed copy is validated again immediately before
//! it is handed to the JVM.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::Digest;
use tauri::{AppHandle, Manager};
use zip::ZipArchive;

use crate::error::{NimbusError, Result};
use crate::paths;

const ARTIFACT_DIR: &str = "nimbus";
const ARTIFACT_FILE: &str = "nimbus-runtime.jar";
const MANIFEST_FILE: &str = "nimbus-runtime.json";
const MANIFEST_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactManifest {
    pub schema: u32,
    pub client_version: String,
    pub minecraft_versions: Vec<String>,
    pub launcher_version: Option<String>,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct ValidatedArtifact {
    pub jar: PathBuf,
    pub manifest: ArtifactManifest,
}

fn artifact_paths(dir: &Path) -> (PathBuf, PathBuf) {
    (dir.join(ARTIFACT_FILE), dir.join(MANIFEST_FILE))
}

fn invalid(reason: impl Into<String>) -> NimbusError {
    NimbusError::NimbusArtifact {
        reason: reason.into(),
    }
}

fn artifact_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn unique_temp_dir(shared_dir: &Path, prefix: &str) -> Result<PathBuf> {
    static NEXT_ID: OnceLock<Mutex<u64>> = OnceLock::new();
    let counter = NEXT_ID.get_or_init(|| Mutex::new(0));
    for _ in 0..100 {
        let id = {
            let mut next = counter
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *next = next.wrapping_add(1);
            *next
        };
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = shared_dir.join(format!(".{prefix}-{}-{timestamp}-{id}", std::process::id()));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(invalid(format!(
        "не удалось получить уникальный каталог runtime Nimbus: {prefix}"
    )))
}

pub struct ArtifactLock(MutexGuard<'static, ()>);

pub fn acquire_lock() -> ArtifactLock {
    ArtifactLock(artifact_lock())
}

fn rollback_directory(from: &Path, to: &Path) -> Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if to.exists() {
        fs::remove_dir_all(to).map_err(|error| {
            invalid(format!(
                "не удалось удалить повреждённый runtime Nimbus {} перед восстановлением: {error}",
                to.display()
            ))
        })?;
    }
    fs::rename(from, to).map_err(|error| {
        invalid(format!(
            "не удалось восстановить рабочий runtime Nimbus {}: {error}",
            to.display()
        ))
    })
}

fn recover_previous(shared_dir: &Path, installed_dir: &Path) -> Result<()> {
    if installed_dir.exists() {
        return Ok(());
    }

    let mut candidates = fs::read_dir(shared_dir)
        .map_err(|error| invalid(format!("не удалось просмотреть runtime Nimbus: {error}")))?
        .flatten()
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".nimbus-runtime-previous-")
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::UNIX_EPOCH)
    });

    if let Some(previous) = candidates.into_iter().last() {
        rollback_directory(&previous.path(), installed_dir)?;
    }
    Ok(())
}

fn ensure_available_locked(app: &AppHandle, game_version: &str) -> Result<ValidatedArtifact> {
    let shared_dir = paths::shared_dir()?;
    fs::create_dir_all(&shared_dir).map_err(|error| {
        invalid(format!(
            "не удалось создать каталог runtime Nimbus {}: {error}",
            shared_dir.display()
        ))
    })?;
    let installed_dir = shared_dir.join(ARTIFACT_DIR);
    recover_previous(&shared_dir, &installed_dir)?;

    match validate_pair(&installed_dir, game_version) {
        Ok(artifact) => {
            // Обновление лаунчера может нести более новый runtime: если упакованная
            // копия отличается от установленной, заменяем установленную. Без этого
            // игра вечно получает первый когда-то установленный клиент.
            match locate_packaged(app, game_version) {
                Ok(packaged) if packaged.manifest.sha256 != artifact.manifest.sha256 => {
                    stage_packaged(&packaged, game_version)
                }
                _ => Ok(artifact),
            }
        }
        Err(installed_error) => match locate_packaged(app, game_version) {
            Ok(packaged) => stage_packaged(&packaged, game_version),
            Err(packaged_error) => Err(invalid(format!(
                "Nimbus Client недоступен для Minecraft {}. Восстановите установку клиента. Причины: {}; {}",
                game_version, installed_error, packaged_error
            ))),
        },
    }
}

pub fn ensure_available(app: &AppHandle, game_version: &str) -> Result<ValidatedArtifact> {
    let _lock = acquire_lock();
    ensure_available_locked(app, game_version)
}

pub fn ensure_available_for_launch(
    app: &AppHandle,
    game_version: &str,
) -> Result<(ValidatedArtifact, ArtifactLock)> {
    let lock = acquire_lock();
    let artifact = ensure_available_locked(app, game_version)?;
    Ok((artifact, lock))
}

impl std::fmt::Debug for ArtifactLock {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ArtifactLock")
            .finish_non_exhaustive()
    }
}

impl ArtifactLock {
    pub fn hold(&self) {}
}

impl Drop for ArtifactLock {
    fn drop(&mut self) {
        let _ = &self.0;
    }
}

fn unique_stage_path(shared_dir: &Path, prefix: &str) -> Result<PathBuf> {
    let path = unique_temp_dir(shared_dir, prefix)?;
    fs::create_dir(&path).map_err(|error| {
        invalid(format!(
            "не удалось подготовить каталог runtime Nimbus {}: {error}",
            path.display()
        ))
    })?;
    Ok(path)
}

fn stage_packaged(packaged: &ValidatedArtifact, game_version: &str) -> Result<ValidatedArtifact> {
    let shared_dir = paths::shared_dir()?;
    fs::create_dir_all(&shared_dir).map_err(|error| {
        invalid(format!(
            "не удалось создать каталог runtime Nimbus {}: {error}",
            shared_dir.display()
        ))
    })?;

    let installed_dir = shared_dir.join(ARTIFACT_DIR);
    let stage_dir = unique_stage_path(&shared_dir, "nimbus-runtime-stage")?;
    let backup_dir = unique_temp_dir(&shared_dir, "nimbus-runtime-previous")?;

    let (staged_jar, staged_manifest) = artifact_paths(&stage_dir);
    let stage_result = (|| {
        fs::copy(&packaged.jar, &staged_jar)
            .map_err(|error| invalid(format!("не удалось подготовить runtime Nimbus: {error}")))?;
        let manifest_json = serde_json::to_vec_pretty(&packaged.manifest)?;
        fs::write(&staged_manifest, manifest_json)
            .map_err(|error| invalid(format!("не удалось подготовить манифест Nimbus: {error}")))?;
        validate_pair(&stage_dir, game_version)
    })();
    if let Err(error) = stage_result {
        let _ = fs::remove_dir_all(&stage_dir);
        return Err(error);
    }

    if installed_dir.exists() {
        if let Err(error) = fs::rename(&installed_dir, &backup_dir) {
            let _ = fs::remove_dir_all(&stage_dir);
            return Err(invalid(format!(
                "не удалось сохранить рабочий runtime Nimbus: {error}"
            )));
        }
    }
    if let Err(error) = fs::rename(&stage_dir, &installed_dir) {
        let rollback = rollback_directory(&backup_dir, &installed_dir).err();
        let _ = fs::remove_dir_all(&stage_dir);
        return Err(invalid(match rollback {
            Some(rollback_error) => {
                format!("не удалось активировать runtime Nimbus: {error}; {rollback_error}")
            }
            None => format!("не удалось активировать runtime Nimbus: {error}"),
        }));
    }

    match validate_pair(&installed_dir, game_version) {
        Ok(artifact) => {
            if let Err(error) = fs::remove_dir_all(&backup_dir) {
                if backup_dir.exists() {
                    return Err(invalid(format!(
                        "runtime Nimbus активирован, но резервная копия не удалена: {error}"
                    )));
                }
            }
            Ok(artifact)
        }
        Err(error) => {
            let remove_error = fs::remove_dir_all(&installed_dir).err();
            let rollback_error = rollback_directory(&backup_dir, &installed_dir).err();
            match (remove_error, rollback_error) {
                (Some(remove), Some(rollback)) => Err(invalid(format!(
                    "runtime Nimbus не прошёл финальную проверку: {error}; не удалось удалить активный каталог: {remove}; не удалось восстановить резервную копию: {rollback}"
                ))),
                (Some(remove), None) => Err(invalid(format!(
                    "runtime Nimbus не прошёл финальную проверку: {error}; не удалось удалить активный каталог: {remove}"
                ))),
                (None, Some(rollback)) => Err(invalid(format!(
                    "runtime Nimbus не прошёл финальную проверку: {error}; не удалось восстановить резервную копию: {rollback}"
                ))),
                (None, None) => Err(error),
            }
        }
    }
}

fn validate_manifest(manifest: &ArtifactManifest, game_version: &str) -> Result<()> {
    if manifest.schema != MANIFEST_SCHEMA {
        return Err(invalid(format!(
            "неподдерживаемая схема манифеста Nimbus: {}",
            manifest.schema
        )));
    }
    if manifest.client_version.trim().is_empty() {
        return Err(invalid("манифест Nimbus не содержит версии клиента"));
    }
    if manifest.minecraft_versions.is_empty()
        || !manifest
            .minecraft_versions
            .iter()
            .any(|v| v == game_version)
    {
        return Err(invalid(format!(
            "Nimbus Client {} не поддерживает Minecraft {}",
            manifest.client_version, game_version
        )));
    }
    if manifest.size == 0 {
        return Err(invalid("манифест Nimbus указывает пустой runtime"));
    }
    if manifest.sha256.len() != 64 || !manifest.sha256.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(invalid("манифест Nimbus содержит неверный SHA-256"));
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Result<ArtifactManifest> {
    let raw = fs::read_to_string(path).map_err(|error| {
        invalid(format!(
            "не удалось прочитать манифест Nimbus {}: {error}",
            path.display()
        ))
    })?;
    serde_json::from_str(&raw).map_err(|error| {
        invalid(format!(
            "манифест Nimbus повреждён {}: {error}",
            path.display()
        ))
    })
}

fn sha256_file(path: &Path) -> Result<(u64, String)> {
    let mut file = fs::File::open(path).map_err(|error| {
        invalid(format!(
            "не удалось открыть runtime Nimbus {}: {error}",
            path.display()
        ))
    })?;
    let mut hasher = sha2::Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer).map_err(|error| {
            invalid(format!(
                "не удалось проверить runtime Nimbus {}: {error}",
                path.display()
            ))
        })?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((size, format!("{:x}", hasher.finalize())))
}

/// Validates one jar/manifest pair against the requested Minecraft version.
pub fn validate_pair(dir: &Path, game_version: &str) -> Result<ValidatedArtifact> {
    let (jar, manifest_path) = artifact_paths(dir);
    if !jar.is_file() {
        return Err(invalid(format!(
            "runtime Nimbus не найден: {}",
            jar.display()
        )));
    }
    if !manifest_path.is_file() {
        return Err(invalid(format!(
            "манифест Nimbus не найден: {}",
            manifest_path.display()
        )));
    }

    let manifest = read_manifest(&manifest_path)?;
    validate_manifest(&manifest, game_version)?;
    let (size, sha256) = sha256_file(&jar)?;
    if size != manifest.size {
        return Err(invalid(format!(
            "размер runtime Nimbus не совпадает: ожидалось {}, получено {}",
            manifest.size, size
        )));
    }
    if !sha256.eq_ignore_ascii_case(&manifest.sha256) {
        return Err(invalid(format!(
            "SHA-256 runtime Nimbus не совпадает: ожидалось {}, получено {}",
            manifest.sha256, sha256
        )));
    }
    validate_jar_manifest(&jar, &manifest.client_version)?;

    Ok(ValidatedArtifact { jar, manifest })
}

fn validate_jar_manifest(jar: &Path, client_version: &str) -> Result<()> {
    let file = fs::File::open(jar).map_err(|error| {
        invalid(format!(
            "не удалось открыть манифест runtime Nimbus {}: {error}",
            jar.display()
        ))
    })?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        invalid(format!(
            "runtime Nimbus не является корректным JAR-архивом {}: {error}",
            jar.display()
        ))
    })?;
    let mut entry = archive.by_name("META-INF/MANIFEST.MF").map_err(|error| {
        invalid(format!(
            "в runtime Nimbus отсутствует META-INF/MANIFEST.MF: {error}"
        ))
    })?;
    let mut raw = String::new();
    entry.read_to_string(&mut raw).map_err(|error| {
        invalid(format!(
            "не удалось прочитать манифест runtime Nimbus {}: {error}",
            jar.display()
        ))
    })?;

    let attribute = |name: &str| -> Option<String> {
        let mut value: Option<String> = None;
        for line in raw.lines() {
            if line.starts_with(' ') {
                if let Some(current) = value.as_mut() {
                    current.push_str(line.trim_start());
                }
                continue;
            }
            let Some((key, line_value)) = line.split_once(':') else {
                continue;
            };
            if key.trim().eq_ignore_ascii_case(name) {
                value = Some(line_value.trim_start().to_owned());
            }
        }
        value
    };

    for (name, expected) in [
        ("Premain-Class", "dev.nimbus.runtime.NimbusAgent"),
        ("Agent-Class", "dev.nimbus.runtime.NimbusAgent"),
        ("Can-Retransform-Classes", "true"),
    ] {
        if attribute(name).as_deref() != Some(expected) {
            return Err(invalid(format!(
                "манифест runtime Nimbus не содержит {name}: {expected}"
            )));
        }
    }
    if attribute("Implementation-Version").as_deref() != Some(client_version) {
        return Err(invalid(format!(
            "версия runtime Nimbus в JAR не совпадает с манифестом: ожидалась {client_version}"
        )));
    }
    Ok(())
}

fn resource_candidates(app: &AppHandle) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join(ARTIFACT_DIR));
        candidates.push(resource_dir.join("resources").join(ARTIFACT_DIR));
    }
    if let Some(path) = std::env::var_os("NIMBUS_RUNTIME_DIR") {
        candidates.push(PathBuf::from(path));
    }
    #[cfg(debug_assertions)]
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(ARTIFACT_DIR),
    );
    candidates
}

fn locate_packaged(app: &AppHandle, game_version: &str) -> Result<ValidatedArtifact> {
    let mut failures = Vec::new();
    for dir in resource_candidates(app) {
        match validate_pair(&dir, game_version) {
            Ok(artifact) => return Ok(artifact),
            Err(error) => failures.push(format!("{}: {error}", dir.display())),
        }
    }
    let detail = if failures.is_empty() {
        "ресурсный каталог Tauri недоступен".to_owned()
    } else {
        failures.join("; ")
    };
    Err(invalid(format!(
        "упакованный runtime Nimbus недоступен — переустановите или восстановите клиент ({detail})"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn fixture(dir: &Path, body: &[u8], versions: &[&str]) {
        fs::create_dir_all(dir).unwrap();
        let jar = dir.join(ARTIFACT_FILE);
        let file = fs::File::create(&jar).unwrap();
        let mut archive = ZipWriter::new(file);
        archive
            .start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(
                b"Manifest-Version: 1.0\r\nPremain-Class: dev.nimbus.runtime.NimbusAgent\r\nAgent-Class: dev.nimbus.runtime.NimbusAgent\r\nCan-Retransform-Classes: true\r\nImplementation-Version: 0.1.0\r\n\r\n",
            )
            .unwrap();
        archive
            .start_file("runtime.bin", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(body).unwrap();
        archive.finish().unwrap();
        let hash = sha256_file(&jar).unwrap();
        let manifest = ArtifactManifest {
            schema: MANIFEST_SCHEMA,
            client_version: "0.1.0".to_owned(),
            minecraft_versions: versions.iter().map(|v| (*v).to_owned()).collect(),
            launcher_version: None,
            sha256: hash.1,
            size: hash.0,
        };
        fs::write(
            dir.join(MANIFEST_FILE),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn validates_matching_manifest_and_hash() {
        let dir = std::env::temp_dir().join(format!("nimbus-artifact-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fixture(&dir, b"runtime", &["1.20.1"]);
        let artifact = validate_pair(&dir, "1.20.1").unwrap();
        assert_eq!(artifact.manifest.client_version, "0.1.0");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_hash_mismatch() {
        let dir = std::env::temp_dir().join(format!("nimbus-artifact-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fixture(&dir, b"runtime", &["1.20.1"]);
        fs::write(dir.join(ARTIFACT_FILE), b"changed").unwrap();
        let error = validate_pair(&dir, "1.20.1").unwrap_err().to_string();
        assert!(error.contains("SHA-256") || error.contains("размер"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_unlisted_game_version() {
        let dir =
            std::env::temp_dir().join(format!("nimbus-artifact-version-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fixture(&dir, b"runtime", &["1.20.1"]);
        let error = validate_pair(&dir, "1.21.1").unwrap_err().to_string();
        assert!(error.contains("не поддерживает"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_non_jar_runtime() {
        let dir = std::env::temp_dir().join(format!("nimbus-artifact-zip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fixture(&dir, b"runtime", &["1.20.1"]);
        fs::write(dir.join(ARTIFACT_FILE), b"not a jar").unwrap();
        let (size, sha256) = sha256_file(&dir.join(ARTIFACT_FILE)).unwrap();
        let mut manifest: ArtifactManifest =
            serde_json::from_slice(&fs::read(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
        manifest.size = size;
        manifest.sha256 = sha256;
        fs::write(
            dir.join(MANIFEST_FILE),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let error = validate_pair(&dir, "1.20.1").unwrap_err().to_string();
        assert!(error.contains("JAR-архивом"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validates_a_requested_version_that_is_not_first() {
        let dir =
            std::env::temp_dir().join(format!("nimbus-artifact-order-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fixture(&dir, b"runtime", &["1.21.1", "1.20.1"]);
        assert!(validate_pair(&dir, "1.20.1").is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn temporary_stage_paths_are_unique_and_reserved() {
        let root =
            std::env::temp_dir().join(format!("nimbus-artifact-stage-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let first = unique_stage_path(&root, "nimbus-runtime-stage").unwrap();
        let second = unique_stage_path(&root, "nimbus-runtime-stage").unwrap();
        assert_ne!(first, second);
        assert!(first.is_dir());
        assert!(second.is_dir());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn recovers_a_previous_runtime_when_the_active_directory_is_missing() {
        let root = std::env::temp_dir().join(format!(
            "nimbus-artifact-recovery-test-{}",
            std::process::id()
        ));
        let installed = root.join(ARTIFACT_DIR);
        let previous = root.join(".nimbus-runtime-previous-recovery");
        fs::create_dir_all(&previous).unwrap();
        fs::write(previous.join("marker"), b"previous").unwrap();

        recover_previous(&root, &installed).unwrap();

        assert_eq!(fs::read(installed.join("marker")).unwrap(), b"previous");
        assert!(!previous.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn artifact_lock_serializes_replacements() {
        use std::sync::mpsc;
        use std::time::Duration;

        let first = acquire_lock();
        let (ready_tx, ready_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _second = acquire_lock();
            ready_tx.send(()).unwrap();
        });

        assert!(ready_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        assert!(ready_rx.recv_timeout(Duration::from_secs(1)).is_ok());
        worker.join().unwrap();
    }
}
