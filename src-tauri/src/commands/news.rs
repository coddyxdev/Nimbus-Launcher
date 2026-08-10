//! Launcher news.
//!
//! The feed lives in the public repository rather than in the binary, so a new
//! post does not need a new release: editing `news.json` on `main` is enough.

use serde::{Deserialize, Serialize};

use crate::download;
use crate::error::{NimbusError, Result};

const NEWS_URL: &str =
    "https://raw.githubusercontent.com/coddyxdev/Nimbus-Launcher/main/news.json";

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

#[tauri::command]
pub async fn fetch_news() -> Result<Vec<NewsItem>> {
    let response = download::client()
        .get(NEWS_URL)
        .send()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))?;

    // A missing file is not an error: the feed is simply empty until the
    // first post is published, so the UI shows its empty state instead of a
    // network failure.
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }

    if !response.status().is_success() {
        return Err(NimbusError::Network(format!(
            "news request failed: HTTP {}",
            response.status()
        )));
    }

    response
        .json::<Vec<NewsItem>>()
        .await
        .map_err(|e| NimbusError::Network(e.to_string()))
}
