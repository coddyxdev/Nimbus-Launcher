//! Discord Rich Presence.
//!
//! The IPC client is synchronous and talks to a local named pipe, so every call
//! is wrapped in `spawn_blocking`. Discord not running is the normal case, not
//! an error: the client is simply absent and every call becomes a no-op, so a
//! missing Discord can never delay or break a game launch.
//!
//! Every failure path below prints a `[nimbus]` line to stderr (same
//! convention as the rest of the backend) so a broken presence is actually
//! diagnosable instead of failing completely silently. None of these failures
//! are surfaced to the user or allowed to affect the game launch itself --
//! Rich Presence is purely cosmetic.
//!
//! Presence is shown the whole time Nimbus Client is open, not only while a
//! game is running: `set_idle` is published on app startup and again right
//! after a game exits, and `set_playing` takes over for the duration of an
//! actual game session.
//!
//! If Discord is running and connects successfully but nothing ever shows up
//! on your profile, that is not a bug in this file: it almost always means
//! the `large_image` key (`"nimbus_logo"`) has not been uploaded as a Rich
//! Presence art asset for this application id in the Discord Developer
//! Portal, or that "Display current activity as a status message" is turned
//! off in Discord's own Activity Privacy settings. Discord silently drops
//! activities in both cases.

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

/// Connects on first use, or reconnects after a previous call found the pipe
/// dead. Returns false when Discord is unavailable -- that is the common case
/// (Discord simply is not running) and is logged as a one-line notice, not an
/// error.
fn ensure_connected(slot: &mut Option<DiscordIpcClient>) -> bool {
    if slot.is_some() {
        return true;
    }
    let mut fresh = DiscordIpcClient::new(APP_ID);
    if let Err(err) = fresh.connect() {
        crate::nlog!("Discord RPC: not connected ({err}); Discord is probably not running");
        return false;
    }
    crate::nlog!("Discord RPC: connected");
    *slot = Some(fresh);
    true
}

/// Builds and sends one activity update, retrying once after a fresh
/// reconnect if the pipe turned out to be dead. Meant to run inside
/// `spawn_blocking`, since the underlying IPC call is synchronous.
fn publish(top_line: &str, second_line: &str, started_at: i64) {
    let mut guard = lock(client());
    let mut attempts_left: u8 = 2;
    loop {
        if attempts_left == 0 {
            return;
        }
        attempts_left -= 1;
        if !ensure_connected(&mut guard) {
            return;
        }
        let Some(ipc) = guard.as_mut() else { return };

        let payload = activity::Activity::new()
            .details(top_line)
            .state(second_line)
            .assets(
                activity::Assets::new()
                    .large_image("nimbus_logo")
                    .large_text("Nimbus Client"),
            )
            .timestamps(activity::Timestamps::new().start(started_at));

        match ipc.set_activity(payload) {
            Ok(()) => return,
            Err(err) => {
                crate::nlog!("Discord RPC: set_activity failed ({err}), reconnecting");
                // A dead pipe (Discord closed or restarted) drops the client
                // so the loop above reconnects instead of leaving Rich
                // Presence broken for the rest of the run.
                *guard = None;
            }
        }
    }
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Publishes "playing <instance>" with an elapsed timer.
///
/// `started_at` is unix seconds, which is what Discord renders as a live
/// counter -- we do not have to keep updating the activity afterwards.
pub async fn set_playing(instance_name: String, details: String, started_at: i64) {
    let _ =
        tokio::task::spawn_blocking(move || publish(&instance_name, &details, started_at)).await;
}

/// Publishes an idle "in the launcher" status. Called on startup and again
/// after a game exits, so Discord reflects that Nimbus Client is open even
/// when no game is currently running, not only while actually playing.
pub async fn set_idle() {
    let started_at = now_epoch();
    let _ = tokio::task::spawn_blocking(move || publish("Nimbus Client", "В лаунчере", started_at))
        .await;
}

/// Clears the activity entirely, e.g. when Rich Presence is turned off in
/// settings.
pub async fn clear() {
    let _ = tokio::task::spawn_blocking(|| {
        let mut guard = lock(client());
        let Some(ipc) = guard.as_mut() else { return };
        if let Err(err) = ipc.clear_activity() {
            crate::nlog!("Discord RPC: clear_activity failed ({err})");
            *guard = None;
        }
    })
    .await;
}
