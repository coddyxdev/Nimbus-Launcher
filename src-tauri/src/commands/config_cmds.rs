//! Bootstrap and configuration commands.

use serde::{Deserialize, Serialize};

use crate::config::{self, Config, Theme};
use crate::error::Result;
use crate::paths;
use crate::presence;

use super::shared::validate_username;

/// Everything the frontend needs on first paint, in one round trip.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    config: Config,
    launcher_version: String,
    data_dir: String,
    auth_unavailable: bool,
}

#[tauri::command]
pub fn bootstrap() -> Result<Bootstrap> {
    let config = config::load()?;
    let data_dir = paths::root()?.to_string_lossy().to_string();
    // Sign-in works out of the box with the id the launcher ships with; a
    // user-supplied client id only overrides it.
    let auth_unavailable = !crate::auth::sign_in_available(config.azure_client_id.as_deref());

    Ok(Bootstrap {
        config,
        launcher_version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir,
        auth_unavailable,
    })
}

#[tauri::command]
pub fn set_theme(theme: Theme) -> Result<Config> {
    config::update(|cfg| {
        cfg.theme = theme;
        Ok(())
    })
}

#[tauri::command]
pub fn set_offline_username(username: String) -> Result<Config> {
    let username = validate_username(&username)?;
    config::update(|cfg| {
        cfg.offline_username = Some(username);
        Ok(())
    })
}

#[tauri::command]
pub fn complete_onboarding() -> Result<Config> {
    config::update(|cfg| {
        cfg.onboarding_done = true;
        Ok(())
    })
}

/// General-purpose config update. Accepts partial fields.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigUpdate {
    theme: Option<Theme>,
    default_memory_mib: Option<u32>,
    default_jvm_args: Option<Vec<String>>,
    default_aikar_flags: Option<bool>,
    offline_username: Option<String>,
    /// Explicit Java path. An empty string clears the override.
    java_path: Option<String>,
    /// Game window size. Zero clears the override.
    game_width: Option<u32>,
    game_height: Option<u32>,
    game_fullscreen: Option<bool>,
    discord_rpc: Option<bool>,
    /// Background strength in percent; clamped to 1..=100.
    background_opacity: Option<u8>,
    /// Background blur radius in pixels; clamped to 0..=40.
    background_blur: Option<u8>,
}

#[tauri::command]
pub fn update_config(update: ConfigUpdate) -> Result<Config> {
    // Read before the closure below moves `update` -- Option<bool> is Copy,
    // so this does not steal the field from the closure.
    let rpc_update = update.discord_rpc;

    let cfg = config::update(|cfg| {
        if let Some(theme) = update.theme {
            cfg.theme = theme;
        }
        if let Some(mem) = update.default_memory_mib {
            cfg.default_memory_mib = mem.clamp(512, 65_536);
        }
        if let Some(args) = update.default_jvm_args {
            cfg.default_jvm_args = args;
        }
        if let Some(aikar) = update.default_aikar_flags {
            cfg.default_aikar_flags = aikar;
        }
        // An invalid nickname is reported instead of being silently ignored.
        if let Some(username) = update.offline_username {
            cfg.offline_username = Some(validate_username(&username)?);
        }
        // An empty path means "go back to auto-detection".
        if let Some(java_path) = update.java_path {
            let trimmed = java_path.trim();
            cfg.java_path = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            };
        }
        // Zero means "let Minecraft decide"; anything else is clamped to a sane
        // window size so a typo cannot produce an invisible window.
        if let Some(width) = update.game_width {
            cfg.game_width = if width == 0 {
                None
            } else {
                Some(width.clamp(320, 15_360))
            };
        }
        if let Some(height) = update.game_height {
            cfg.game_height = if height == 0 {
                None
            } else {
                Some(height.clamp(240, 8_640))
            };
        }
        if let Some(fullscreen) = update.game_fullscreen {
            cfg.game_fullscreen = fullscreen;
        }
        if let Some(rpc) = update.discord_rpc {
            cfg.discord_rpc = rpc;
        }
        // Zero opacity would look like a bug rather than a setting: the
        // picture would vanish while the toggle still says a background is on.
        if let Some(opacity) = update.background_opacity {
            cfg.background_opacity = opacity.clamp(1, 100);
        }
        if let Some(blur) = update.background_blur {
            cfg.background_blur = blur.min(40);
        }
        Ok(())
    })?;

    // React to the Rich Presence toggle immediately instead of waiting for
    // the next game launch or app restart to take effect.
    if let Some(rpc_enabled) = rpc_update {
        tauri::async_runtime::spawn(async move {
            if rpc_enabled {
                presence::set_idle().await;
            } else {
                presence::clear().await;
            }
        });
    }

    Ok(cfg)
}
