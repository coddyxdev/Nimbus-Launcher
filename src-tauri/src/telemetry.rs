//! Lightweight, anonymous telemetry for counting active users.
//!
//! On every launcher start the module POSTs a tiny JSON payload to a
//! configurable endpoint.  The payload carries **no** personal data:
//!
//! - `anonymous_id` — a random UUID v4 generated once and stored in
//!   `config.json`.  It never leaves the machine when telemetry is off.
//! - `version`      — the launcher's Cargo package version.
//! - `platform`     — always `"windows"` (the only supported platform).
//! - `ts`           — ISO-8601 UTC timestamp of this ping.
//!
//! The whole thing is fire-and-forget: a background task that is spawned
//! during `setup` and never awaited.  Failures are silently swallowed —
//! telemetry must never slow down or block the UI.
//!
//! ## Endpoint
//!
//! The developer runs a tiny HTTP service that receives these pings and
//! aggregates unique `anonymous_id`s per day / week / month.  A static
//! JSON file like `stats.json` or a ClickHouse / SQLite warehouse works
//! fine — the launcher does not care what happens on the server side.

use serde::Serialize;
use serde_json::json;

use crate::config;

/// Telemetry endpoint URL.  Change this when you move the backend.
const TELEMETRY_URL: &str = "https://nimbus-stats.coddyxdev.com/ping";

/// Fire-and-forget startup ping.  Called once during `setup` inside a
/// detached `tokio::spawn`.  Never awaits — returns immediately.
pub async fn send_startup_ping() {
    // Fast path: telemetry is disabled or config cannot be read.
    let (enabled, anonymous_id, version) = match config::load() {
        Ok(cfg) if cfg.telemetry => {
            let id = match config::ensure_anonymous_id() {
                Ok(id) => id,
                Err(_) => return, // cannot generate an id — skip silently
            };
            (true, id, env!("CARGO_PKG_VERSION").to_string())
        }
        _ => return,
    };

    if !enabled {
        return;
    }

    let payload = json!({
        "anonymous_id": anonymous_id,
        "version": version,
        "platform": "windows",
        "ts": chrono::Utc::now().to_rfc3339(),
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(_) => return,
    };

    let _ = client
        .post(TELEMETRY_URL)
        .header("Content-Type", "application/json")
        .body(payload.to_string())
        .send()
        .await;
}

/// Summary returned by the `get_telemetry_stats` command.
/// Aggregated server-side; the launcher only displays it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryStats {
    /// Whether telemetry is currently enabled in the local config.
    pub enabled: bool,
    /// The persistent anonymous id (safe to display — it is random, not PII).
    pub anonymous_id: Option<String>,
}
