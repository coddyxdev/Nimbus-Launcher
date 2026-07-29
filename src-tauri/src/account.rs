//! Persistence for the signed-in Microsoft account.
//!
//! Kept out of `config.json` on purpose: the config is handed to the WebView on
//! every boot, and tokens have no business being there. This file holds the
//! secrets, and only a sanitised [`AccountInfo`] ever crosses the IPC boundary.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::auth::{self, AuthenticatedAccount};
use crate::config::{self, write_atomic};
use crate::error::{NimbusError, Result};
use crate::paths;

/// On-disk shape. Tokens are stored in plain text, like every other launcher
/// that supports Microsoft login: the game process needs them, and Windows
/// user-profile permissions are the actual boundary here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAccount {
    pub uuid: String,
    pub name: String,
    pub mc_access_token: String,
    pub mc_expires_at: u64,
    pub ms_refresh_token: String,
}

/// What the UI is allowed to see.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub uuid: String,
    pub name: String,
    /// Unix seconds; the UI uses it only to say "session expires soon".
    pub expires_at: u64,
}

impl StoredAccount {
    pub fn info(&self) -> AccountInfo {
        AccountInfo {
            uuid: self.uuid.clone(),
            name: self.name.clone(),
            expires_at: self.mc_expires_at,
        }
    }
}

impl From<AuthenticatedAccount> for StoredAccount {
    fn from(value: AuthenticatedAccount) -> Self {
        Self {
            uuid: value.uuid,
            name: value.name,
            mc_access_token: value.mc_access_token,
            mc_expires_at: value.mc_expires_at,
            ms_refresh_token: value.ms_refresh_token,
        }
    }
}

fn account_file() -> Result<PathBuf> {
    Ok(paths::root()?.join("account.json"))
}

/// Reads the stored account, or `None` when nobody is signed in.
///
/// A corrupt file is treated as "not signed in" rather than an error: the user
/// can always sign in again, and refusing to start would be worse.
pub fn load() -> Result<Option<StoredAccount>> {
    let path = account_file()?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw).ok())
}

pub fn save(account: &StoredAccount) -> Result<()> {
    let path = account_file()?;
    let body = serde_json::to_vec_pretty(account)?;
    write_atomic(&path, &body)
}

pub fn clear() -> Result<()> {
    let path = account_file()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Returns a usable account, refreshing the Minecraft token when it is stale.
///
/// Called right before launching. If the refresh token itself has been revoked
/// the account is dropped, so the UI reflects that a new sign-in is needed
/// instead of failing on every launch.
pub async fn valid_account() -> Result<Option<StoredAccount>> {
    let Some(account) = load()? else {
        return Ok(None);
    };
    if !auth::token_stale(account.mc_expires_at) {
        return Ok(Some(account));
    }

    let cfg = config::load()?;
    let client_id = cfg.azure_client_id.unwrap_or_default();
    if client_id.trim().is_empty() {
        return Err(NimbusError::Invalid(
            "Сессия Microsoft истекла, а Azure Client ID не задан".to_owned(),
        ));
    }

    match auth::refresh_tokens(&client_id, &account.ms_refresh_token).await {
        Ok(tokens) => {
            let refreshed: StoredAccount = auth::finish_login(tokens).await?.into();
            save(&refreshed)?;
            Ok(Some(refreshed))
        }
        Err(err) => {
            // Revoked or expired beyond recovery: forget it and report cleanly.
            clear()?;
            Err(NimbusError::Invalid(format!(
                "Сессия Microsoft истекла, войдите заново ({err})"
            )))
        }
    }
}
