//! Ely.by account commands: username+password sign-in, multi-account state.
//!
//! Signing in only stores a session (see `ely_account.rs`); the game itself
//! only actually shows Ely.by skins and multiplayer works once launch also
//! attaches `authlib-injector` -- wired in `commands/launch.rs` whenever
//! `config.active_identity` is `Ely`.

use crate::config::{self, IdentityKind};
use crate::ely;
use crate::ely_account::{self, ElyAccountInfo, StoredElyAccount};
use crate::error::Result;

/// Signs in with an Ely.by username (or email) and password, and makes the
/// account the active identity -- the next launch plays as it, and the
/// account's skin becomes visible to every other Ely.by-configured client.
#[tauri::command]
pub async fn ely_sign_in(username: String, password: String) -> Result<ElyAccountInfo> {
    let tokens = ely::authenticate(username.trim(), &password).await?;
    let info = ely_account::upsert_and_activate(StoredElyAccount {
        uuid: tokens.uuid,
        name: tokens.name,
        access_token: tokens.access_token,
        client_token: tokens.client_token,
    })?;
    config::update(|cfg| {
        cfg.active_identity = IdentityKind::Ely;
        Ok(())
    })?;
    Ok(info)
}

/// Every signed-in Ely.by account, active one first.
#[tauri::command]
pub fn list_ely_accounts() -> Result<Vec<ElyAccountInfo>> {
    ely_account::list()
}

/// Makes an already signed-in Ely.by account active and the active identity.
#[tauri::command]
pub fn switch_ely_account(uuid: String) -> Result<ElyAccountInfo> {
    let info = ely_account::set_active(&uuid)?;
    config::update(|cfg| {
        cfg.active_identity = IdentityKind::Ely;
        Ok(())
    })?;
    Ok(info)
}

/// Removes one signed-in Ely.by account.
#[tauri::command]
pub fn remove_ely_account(uuid: String) -> Result<Option<ElyAccountInfo>> {
    ely_account::remove(&uuid)
}

/// Signs out of Ely.by completely.
#[tauri::command]
pub fn ely_sign_out() -> Result<()> {
    ely_account::clear()
}
