//! Offline ("pirate") profile commands: nicknames that play without signing
//! into a Microsoft account, plus the locally-stored skin each one can have.

use crate::config::{self, IdentityKind};
use crate::error::Result;
use crate::offline::{self, OfflineProfileInfo};
use crate::skin::{self, SkinModel};

/// Every known offline nickname, active one first.
#[tauri::command]
pub fn list_offline_profiles() -> Result<Vec<OfflineProfileInfo>> {
    offline::list()
}

/// Adds (or reuses) an offline nickname and makes it the active identity --
/// the next launch plays as this nickname instead of any signed-in
/// Microsoft account, until a Microsoft account is chosen again.
#[tauri::command]
pub fn add_offline_profile(name: String) -> Result<OfflineProfileInfo> {
    let info = offline::upsert_and_activate(&name)?;
    config::update(|cfg| {
        cfg.active_identity = IdentityKind::Offline;
        Ok(())
    })?;
    Ok(info)
}

/// Switches to an already-known offline nickname and makes offline play the
/// active identity.
#[tauri::command]
pub fn switch_offline_profile(name: String) -> Result<OfflineProfileInfo> {
    let info = offline::set_active(&name)?;
    config::update(|cfg| {
        cfg.active_identity = IdentityKind::Offline;
        Ok(())
    })?;
    Ok(info)
}

/// Removes an offline nickname and its skin, if any.
#[tauri::command]
pub fn remove_offline_profile(name: String) -> Result<Option<OfflineProfileInfo>> {
    offline::remove(&name)
}

/// Sets an offline nickname's skin from a pasted image URL.
#[tauri::command]
pub async fn set_offline_skin_url(
    name: String,
    url: String,
    model: String,
) -> Result<OfflineProfileInfo> {
    let bytes = skin::fetch_skin_bytes(url.trim()).await?;
    offline::set_skin(&name, bytes, SkinModel::parse(&model)).await
}

/// Sets an offline nickname's skin from a file picked on disk.
#[tauri::command]
pub async fn set_offline_skin_file(
    name: String,
    file_path: String,
    model: String,
) -> Result<OfflineProfileInfo> {
    let bytes = skin::read_local_skin(std::path::Path::new(&file_path)).await?;
    offline::set_skin(&name, bytes, SkinModel::parse(&model)).await
}

/// Clears an offline nickname's skin, back to the default Steve/Alex look.
#[tauri::command]
pub async fn clear_offline_skin(name: String) -> Result<OfflineProfileInfo> {
    offline::clear_skin(&name).await
}
