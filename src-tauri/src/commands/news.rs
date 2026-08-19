//! Launcher news.
//!
//! The feed lives in the public repository rather than in the binary, so a new
//! post does not need a new release: editing `news.json` on `main` is enough.
//! A local fallback allows dev builds and offline scenarios to show news too.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::download;
use crate::error::Result;

const NEWS_URL: &str =
    "https://raw.githubusercontent.com/coddyxdev/Nimbus-Launcher/main/news.json";

/// Try to find a local news.json next to the binary or in the project root (dev).
fn local_news_path() -> Option<PathBuf> {
    // In dev mode, try the current working directory first.
    if let Ok(cwd) = std::env::current_dir() {
        eprintln!("[news] cwd = {}", cwd.display());
        let dev = cwd.join("news.json");
        if dev.exists() {
            return Some(dev);
        }
    }
    // Next to the binary (production builds).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            eprintln!("[news] exe dir = {}", dir.display());
            let bin = dir.join("news.json");
            if bin.exists() {
                return Some(bin);
            }
        }
    }
    None
}

/// One post. Both languages travel together so switching the launcher language
/// re-renders instantly without another request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    pub id: String,
    /// ISO date, shown as-is; the feed is written by hand.
    pub date: String,
    pub title_ru: String,
    pub title_en: String,
    pub body_ru: String,
    pub body_en: String,
    /// Optional "read more" target, opened in the system browser.
    #[serde(default)]
    pub link: Option<String>,
}

/// Loads news, trying local file first (dev convenience) then the remote feed.
#[tauri::command]
pub async fn fetch_news() -> Result<Vec<NewsItem>> {
    // Local first: in dev mode this lets you preview news.json edits without
    // pushing to GitHub.
    match local_news_path() {
        Some(path) => {
            eprintln!("[news] local path found: {}", path.display());
            match std::fs::read_to_string(&path) {
                Ok(data) => match serde_json::from_str::<Vec<NewsItem>>(&data) {
                    Ok(items) => {
                        eprintln!("[news] loaded {} items from local", items.len());
                        return Ok(items);
                    }
                    Err(e) => eprintln!("[news] local parse error: {e}"),
                },
                Err(e) => eprintln!("[news] local read error: {e}"),
            }
        }
        None => eprintln!("[news] no local news.json found"),
    }

    // Remote fallback.
    if let Ok(response) = download::client().get(NEWS_URL).send().await {
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if status.is_success() {
            if let Ok(items) = response.json::<Vec<NewsItem>>().await {
                return Ok(items);
            }
        }
    }

    // Nothing worked — empty feed.
    Ok(Vec::new())
}
