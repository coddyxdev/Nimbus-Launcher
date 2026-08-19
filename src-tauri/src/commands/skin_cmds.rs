//! Skin commands for signed-in Microsoft accounts: the real Mojang skin,
//! changed through the same official API the Minecraft Launcher itself
//! uses, so the result shows up for every player on every server.

use crate::account;
use crate::error::{NimbusError, Result};
use crate::skin::{self, PublicSkin, SkinModel};

/// A valid Minecraft access token for the *currently active* Microsoft
/// account, refreshed if needed. Changing a skin is independent of which
/// identity the game actually launches with (`config.active_identity`), so
/// this only requires a signed-in Microsoft account to exist and be active
/// in the account list -- it does not care whether offline play is
/// currently selected for launching.
async fn active_mc_token() -> Result<String> {
    let account = account::valid_account()
        .await?
        .ok_or_else(|| NimbusError::Invalid("Сначала войдите в аккаунт Microsoft".to_owned()))?;
    Ok(account.mc_access_token)
}

/// The active Microsoft account's current real skin, straight from Mojang's
/// public session server -- so the editor can show "here is what you have
/// now" before changing anything. `None` if it is still the default
/// Steve/Alex look, or nobody is signed in.
#[tauri::command]
pub async fn get_active_microsoft_skin() -> Result<Option<PublicSkin>> {
    let Some(account) = account::load()? else {
        return Ok(None);
    };
    skin::fetch_public_skin(&account.uuid).await
}

/// Sets the active Microsoft account's real skin from a pasted image URL.
#[tauri::command]
pub async fn set_microsoft_skin_url(url: String, model: String) -> Result<()> {
    let token = active_mc_token().await?;
    skin::upload_skin_url(&token, url.trim(), SkinModel::parse(&model)).await
}

/// Sets the active Microsoft account's real skin from a file picked on disk.
#[tauri::command]
pub async fn set_microsoft_skin_file(file_path: String, model: String) -> Result<()> {
    let token = active_mc_token().await?;
    let bytes = skin::read_local_skin(std::path::Path::new(&file_path)).await?;
    skin::upload_skin_file(&token, bytes, SkinModel::parse(&model)).await
}

/// Resets the active Microsoft account back to its default Steve/Alex skin.
#[tauri::command]
pub async fn reset_microsoft_skin() -> Result<()> {
    let token = active_mc_token().await?;
    skin::reset_skin(&token).await
}
