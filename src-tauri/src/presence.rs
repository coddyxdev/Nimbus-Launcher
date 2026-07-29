//! Discord Rich Presence.
//!
//! The IPC client is synchronous and talks to a local named pipe, so every call
//! is wrapped in `spawn_blocking`. Discord not running is the normal case, not
//! an error: the client is simply absent and every call becomes a no-op, so a
//! missing Discord can never delay or break a game launch.

use std::sync::Mutex;

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};

use crate::commands::shared::lock;

/// Nimbus Client application id from the Discord developer portal. This is a
/// public identifier, not a secret.
const APP_ID: &str = "1532022279427588096";

fn client() -> &'static Mutex<Option<DiscordIpcClient>> {
    static CLIENT: std::sync::OnceLock<Mutex<Option<DiscordIpcClient>>> =
        std::sync::OnceLock::new();
    CLIENT.get_or_init(|| Mutex::new(None))
}

/// Connects on first use. Returns false when Discord is unavailable.
fn ensure_connected(slot: &mut Option<DiscordIpcClient>) -> bool {
    if slot.is_some() {
        return true;
    }
    let mut fresh = DiscordIpcClient::new(APP_ID);
    if fresh.connect().is_err() {
        return false;
    }
    *slot = Some(fresh);
    true
}

/// Publishes "playing <instance>" with an elapsed timer.
///
/// `started_at` is unix seconds, which is what Discord renders as a live
/// counter — we do not have to keep updating the activity afterwards.
pub async fn set_playing(instance_name: String, details: String, started_at: i64) {
    let _ = tokio::task::spawn_blocking(move || {
        let mut guard = lock(client());
        if !ensure_connected(&mut guard) {
            return;
        }
        let Some(ipc) = guard.as_mut() else { return };

        let payload = activity::Activity::new()
            .details(&instance_name)
            .state(&details)
            .assets(
                activity::Assets::new()
                    .large_image("nimbus_logo")
                    .large_text("Nimbus Client"),
            )
            .timestamps(activity::Timestamps::new().start(started_at));

        // A dead pipe (Discord closed mid-session) drops the client so the next
        // call reconnects instead of failing forever.
        if ipc.set_activity(payload).is_err() {
            *guard = None;
        }
    })
    .await;
}

/// Clears the activity when the game exits.
pub async fn clear() {
    let _ = tokio::task::spawn_blocking(|| {
        let mut guard = lock(client());
        let Some(ipc) = guard.as_mut() else { return };
        if ipc.clear_activity().is_err() {
            *guard = None;
        }
    })
    .await;
}
