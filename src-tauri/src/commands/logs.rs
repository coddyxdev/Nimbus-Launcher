//! Reading the per-instance launcher log.

use crate::error::Result;
use crate::instance;
use crate::paths;

/// Hard cap so a runaway log cannot exhaust memory or freeze the WebView.
const MAX_LINES: usize = 5000;

/// Reads the latest game log for an instance, keeping at most the last
/// [`MAX_LINES`] lines. Returns an empty vec when the log does not exist.
#[tauri::command]
pub async fn get_game_log(instance_id: String) -> Result<Vec<String>> {
    use tokio::io::AsyncBufReadExt;

    let instances_dir = paths::instances_dir()?;
    // Validates the id and rejects traversal attempts.
    let inst = instance::load(&instances_dir, &instance_id)?;
    let log_path = inst.logs_dir(&instances_dir).join("latest.log");

    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let file = tokio::fs::File::open(&log_path).await?;
    let reader = tokio::io::BufReader::new(file);
    // Ring buffer: Vec::remove(0) shifts the whole buffer per line, which
    // makes reading a long log quadratic.
    let mut lines: std::collections::VecDeque<String> =
        std::collections::VecDeque::with_capacity(MAX_LINES);
    let mut stream = reader.lines();
    while let Ok(Some(line)) = stream.next_line().await {
        if lines.len() == MAX_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    Ok(lines.into())
}

/// Absolute path of the current `latest.log`, for "open log folder" and export.
#[tauri::command]
pub async fn game_log_path(instance_id: String) -> Result<String> {
    let instances_dir = paths::instances_dir()?;
    let inst = instance::load(&instances_dir, &instance_id)?;
    Ok(inst
        .logs_dir(&instances_dir)
        .join("latest.log")
        .to_string_lossy()
        .to_string())
}
