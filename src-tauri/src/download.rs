//! Parallel, resumable, hash-verified downloads.
//!
//! One shared `reqwest::Client` (connection pool, separate connect/read
//! timeouts), a global semaphore capping parallel file transfers, bounded
//! retries for transient failures only, SHA-1/SHA-256 verification with a
//! single forced re-download on mismatch, and resume of partial files via
//! HTTP Range. A file whose hash already matches on disk is never fetched.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderValue, RANGE};
use reqwest::StatusCode;
use sha1::Digest as _;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Semaphore, OwnedSemaphorePermit};
use std::sync::Arc;

use crate::error::{NimbusError, Result};

const MAX_PARALLEL: usize = 12;
const POOL_MAX_IDLE: usize = 8;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const RETRY_DELAYS_MS: [u64; 3] = [200, 400, 800];
const IO_BUF: usize = 64 * 1024;
/// Minimum gap between byte-progress events of a single transfer. One event
/// per chunk multiplied by 12 parallel transfers floods the IPC bridge with
/// thousands of messages per second and stalls the WebView.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();

pub fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("NimbusClient/", env!("CARGO_PKG_VERSION")))
            .pool_max_idle_per_host(POOL_MAX_IDLE)
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .build()
            .expect("reqwest client must build")
    })
}

fn permits() -> Arc<Semaphore> {
    PERMITS
        .get_or_init(|| Arc::new(Semaphore::new(MAX_PARALLEL)))
        .clone()
}

/// Expected hash for a downloaded artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedHash {
    Sha1(String),
    Sha256(String),
}

impl ExpectedHash {
    pub fn algorithm(&self) -> &'static str {
        match self {
            Self::Sha1(_) => "sha1",
            Self::Sha256(_) => "sha256",
        }
    }
    pub fn value(&self) -> &str {
        match self {
            Self::Sha1(v) | Self::Sha256(v) => v,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub url: String,
    pub dest: PathBuf,
    pub hash: Option<ExpectedHash>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // `file`/`total` are consumed by the frontend progress UI (Stage 2 UI).
pub enum ProgressEvent {
    /// A new transfer has started (or resumed). `total` is `None` when the
    /// server did not send Content-Length.
    Started { file: String, total: Option<u64> },
    /// `delta` bytes were written to disk since the last event. Coalesced:
    /// one event covers everything written during `PROGRESS_INTERVAL`.
    Bytes { file: String, delta: u64 },
    /// The file is in its final location and verified.
    Finished { file: String },
}

pub type ProgressSender = tokio::sync::mpsc::UnboundedSender<ProgressEvent>;

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Downloaded,
    /// Already on disk with a matching hash — no network traffic.
    Reused,
}

/// `<dest>.tmp` — appended, never replacing the extension.
fn tmp_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

fn status_retriable(status: StatusCode) -> bool {
    status.is_server_error()
}

fn reqwest_to_nimbus(err: reqwest::Error) -> NimbusError {
    if let Some(status) = err.status() {
        NimbusError::Http {
            status: status.as_u16(),
            url: err.url().map(|u| u.to_string()).unwrap_or_default(),
            retriable: status_retriable(status),
        }
    } else {
        NimbusError::Network(err.to_string())
    }
}

/// SHA-1 or SHA-256 hex digest of a file, streamed in 64 KiB chunks.
pub async fn hash_file(path: &Path, algorithm: &str) -> Result<String> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await?;
    let mut buf = vec![0u8; IO_BUF];

    if algorithm == "sha256" {
        let mut h = sha2::Sha256::new();
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            sha2::Digest::update(&mut h, &buf[..n]);
        }
        Ok(format!("{:x}", sha2::Digest::finalize(h)))
    } else {
        let mut h = sha1::Sha1::new();
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            sha1::Digest::update(&mut h, &buf[..n]);
        }
        Ok(format!("{:x}", sha1::Digest::finalize(h)))
    }
}

pub async fn verify(path: &Path, expected: &ExpectedHash) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    Ok(hash_file(path, expected.algorithm()).await? == expected.value())
}

/// One attempt: resume via Range header, write to `.tmp`, sync.
/// Returns `Ok(())` even if the server ignores Range (200 → overwrite).
async fn transfer_once(task: &DownloadTask, progress: &ProgressSender) -> Result<()> {
    let tmp = tmp_path(&task.dest);
    let resume_from = tokio::fs::metadata(&tmp).await.map(|m| m.len()).unwrap_or(0);

    let mut req = client().get(&task.url);
    if resume_from > 0 {
        let range = format!("bytes={resume_from}-");
        req = req.header(
            RANGE,
            HeaderValue::from_str(&range)
                .map_err(|_| NimbusError::Invalid(format!("bad range header for {}", task.url)))?,
        );
    }

    let mut resp = req.send().await.map_err(reqwest_to_nimbus)?;
    let status = resp.status();

    // 416 means our .tmp is already complete or larger than the resource.
    if status == StatusCode::RANGE_NOT_SATISFIABLE && resume_from > 0 {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(NimbusError::Network(format!(
            "stale partial file for {}, restarting",
            task.url
        )));
    }
    if !status.is_success() {
        return Err(NimbusError::Http {
            status: status.as_u16(),
            url: task.url.clone(),
            retriable: status_retriable(status),
        });
    }

    let mut file = if status == StatusCode::PARTIAL_CONTENT && resume_from > 0 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&tmp)
            .await?
    } else {
        tokio::fs::File::create(&tmp).await?
    };

    let file_label = task.dest.to_string_lossy().into_owned();

    let _ = progress.send(ProgressEvent::Started {
        file: file_label.clone(),
        total: task.size,
    });

    // Byte progress is coalesced: bytes are accumulated and reported at most
    // once per PROGRESS_INTERVAL, with a final flush for the remainder.
    let mut pending: u64 = 0;
    let mut last_sent = Instant::now();
    while let Some(chunk) = resp.chunk().await.map_err(reqwest_to_nimbus)? {
        file.write_all(&chunk).await?;
        pending += chunk.len() as u64;
        if last_sent.elapsed() >= PROGRESS_INTERVAL {
            let _ = progress.send(ProgressEvent::Bytes {
                file: file_label.clone(),
                delta: pending,
            });
            pending = 0;
            last_sent = Instant::now();
        }
    }
    if pending > 0 {
        let _ = progress.send(ProgressEvent::Bytes {
            file: file_label,
            delta: pending,
        });
    }
    file.sync_all().await?;
    Ok(())
}

fn nimbus_retriable(err: &NimbusError) -> bool {
    match err {
        NimbusError::Http { retriable, .. } => *retriable,
        NimbusError::Network(_) => true,
        _ => false,
    }
}

/// Downloads one task: dedup by hash, bounded retries, hash verification with
/// a single forced re-download, atomic rename.
pub async fn download_one(task: DownloadTask, progress: ProgressSender) -> Result<Outcome> {
    // Fast path: file already on disk with a matching hash.
    if let Some(hash) = &task.hash {
        if verify(&task.dest, hash).await? {
            let _ = progress.send(ProgressEvent::Finished {
                file: task.dest.to_string_lossy().into_owned(),
            });
            return Ok(Outcome::Reused);
        }
    }

    if let Some(parent) = task.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Attempt with retries.
    let mut last_err: Option<NimbusError> = None;
    for (attempt, &delay_ms) in RETRY_DELAYS_MS.iter().enumerate() {
        match transfer_once(&task, &progress).await {
            Ok(()) => {
                last_err = None;
                break;
            }
            Err(err) => {
                if !nimbus_retriable(&err) {
                    return Err(err);
                }
                let is_last = attempt + 1 == RETRY_DELAYS_MS.len();
                if is_last {
                    return Err(err);
                }
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                last_err = Some(err);
            }
        }
    }
    if let Some(err) = last_err {
        return Err(err);
    }

    // Hash check on the .tmp. One forced re-download on mismatch.
    if let Some(hash) = &task.hash {
        let tmp = tmp_path(&task.dest);
        if !verify(&tmp, hash).await? {
            let _ = tokio::fs::remove_file(&tmp).await;
            transfer_once(&task, &progress).await?;
            if !verify(&tmp, hash).await? {
                let actual = hash_file(&tmp, hash.algorithm()).await?;
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(NimbusError::HashMismatch {
                    path: task.dest.to_string_lossy().into_owned(),
                    algorithm: hash.algorithm().to_string(),
                    expected: hash.value().to_string(),
                    actual,
                });
            }
        }
    }

    tokio::fs::rename(tmp_path(&task.dest), &task.dest).await?;
    let _ = progress.send(ProgressEvent::Finished {
        file: task.dest.to_string_lossy().into_owned(),
    });
    Ok(Outcome::Downloaded)
}

/// Runs a batch concurrently under the global semaphore. The first failure
/// aborts all remaining tasks.
pub async fn download_many(tasks: Vec<DownloadTask>, progress: ProgressSender) -> Result<()> {
    let mut set = tokio::task::JoinSet::new();
    for task in tasks {
        let permit = permits()
            .acquire_owned()
            .await
            .map_err(|_| NimbusError::Invalid("download semaphore closed".into()))?;
        let sender = progress.clone();
        set.spawn(async move {
            let _permit: OwnedSemaphorePermit = permit; // holds slot for the duration
            download_one(task, sender).await
        });
    }

    while let Some(result) = set.join_next().await {
        match result {
            Ok(Ok(_)) => {}
            Ok(Err(err)) => {
                set.abort_all();
                return Err(err);
            }
            Err(join_err) => {
                set.abort_all();
                return Err(NimbusError::Invalid(format!(
                    "download task panicked: {join_err}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_appends_not_replaces_extension() {
        let dest = Path::new("C:/shared/versions/1.20.1/1.20.1.jar");
        assert_eq!(
            tmp_path(dest),
            Path::new("C:/shared/versions/1.20.1/1.20.1.jar.tmp")
        );
    }

    #[test]
    fn only_5xx_retriable() {
        assert!(status_retriable(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(status_retriable(StatusCode::BAD_GATEWAY));
        assert!(!status_retriable(StatusCode::NOT_FOUND));
        assert!(!status_retriable(StatusCode::FORBIDDEN));
        assert!(!status_retriable(StatusCode::OK));
    }

    #[test]
    fn progress_interval_is_not_per_chunk() {
        assert!(PROGRESS_INTERVAL >= Duration::from_millis(50));
    }
}
