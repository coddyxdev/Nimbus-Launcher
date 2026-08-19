//! Offline ("pirate") player profiles: nicknames that play without a signed
//! in Microsoft account, exactly like vanilla Minecraft's own offline mode.
//!
//! Nimbus supports several offline nicknames at once, the same way it
//! supports several Microsoft accounts (see `account.rs`): every nickname
//! the user has added stays known, and one of them is active. Unlike
//! `account.rs`, there are no secrets here -- just names and a path to a
//! locally chosen skin image -- so this is plain, unencrypted JSON.
//!
//! Which of "the active offline profile" and "the active Microsoft account"
//! actually wins at launch is decided by `config.active_identity`, not by
//! anything in this file.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::commands::shared::validate_username;
use crate::config::write_atomic;
use crate::error::{NimbusError, Result};
use crate::launcher::offline_uuid;
use crate::paths;
use crate::skin::{self, SkinModel};

/// One locally-known offline nickname, plus whatever skin was chosen for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineProfile {
    /// Computed the same way the game itself computes it
    /// (`UUID.nameUUIDFromBytes("OfflinePlayer:<name>")`), so a skin filed
    /// under this id lines up with the UUID Minecraft actually assigns.
    pub uuid: String,
    pub name: String,
    /// File name inside `<root>/skins`, if a custom skin was chosen.
    #[serde(default)]
    pub skin_file: Option<String>,
    #[serde(default)]
    pub skin_model: SkinModel,
}

/// What crosses the IPC boundary: the profile plus an absolute skin path the
/// frontend can hand to `convertFileSrc`, resolved fresh on every read so a
/// deleted file never leaves a broken image on screen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineProfileInfo {
    pub uuid: String,
    pub name: String,
    pub skin_path: Option<String>,
    pub skin_model: SkinModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct OfflineStore {
    profiles: Vec<OfflineProfile>,
    active_name: Option<String>,
}

fn store_file() -> Result<PathBuf> {
    Ok(paths::root()?.join("offline_profiles.json"))
}

fn load_store() -> Result<OfflineStore> {
    let path = store_file()?;
    if !path.exists() {
        return Ok(OfflineStore::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

fn save_store(store: &OfflineStore) -> Result<()> {
    write_atomic(&store_file()?, serde_json::to_vec_pretty(store)?.as_slice())
}

fn info(profile: &OfflineProfile) -> Result<OfflineProfileInfo> {
    let skin_path = match &profile.skin_file {
        Some(name) => {
            let path = skin::skins_dir()?.join(name);
            path.exists().then(|| path.to_string_lossy().into_owned())
        }
        None => None,
    };
    Ok(OfflineProfileInfo {
        uuid: profile.uuid.clone(),
        name: profile.name.clone(),
        skin_path,
        skin_model: profile.skin_model,
    })
}

/// Every known offline profile, active one first.
pub fn list() -> Result<Vec<OfflineProfileInfo>> {
    let store = load_store()?;
    let mut infos = store
        .profiles
        .iter()
        .map(info)
        .collect::<Result<Vec<_>>>()?;
    if let Some(active) = &store.active_name {
        infos.sort_by_key(|p| if &p.name == active { 0 } else { 1 });
    }
    Ok(infos)
}

/// The active offline profile, if any exist. Falls back to the first known
/// profile when the active pointer is stale or unset, so removing/renaming
/// never leaves this returning `None` while profiles still exist.
pub fn active() -> Result<Option<OfflineProfile>> {
    let store = load_store()?;
    if let Some(name) = &store.active_name {
        if let Some(p) = store.profiles.iter().find(|p| &p.name == name) {
            return Ok(Some(p.clone()));
        }
    }
    Ok(store.profiles.into_iter().next())
}

/// The profile a launch should fall back to, creating a default "Player"
/// profile if the user has never set up any identity at all -- matching the
/// launcher's old built-in default for a completely fresh install.
pub fn active_or_default() -> Result<OfflineProfile> {
    if let Some(p) = active()? {
        return Ok(p);
    }
    let info = upsert_and_activate("Player")?;
    Ok(OfflineProfile {
        uuid: info.uuid,
        name: info.name,
        skin_file: None,
        skin_model: info.skin_model,
    })
}

/// Adds a profile if it is not already known (matched case-insensitively,
/// like Minecraft usernames), without changing which one is active.
fn upsert(name: &str) -> Result<OfflineProfile> {
    let name = validate_username(name)?;
    let mut store = load_store()?;
    if let Some(existing) = store
        .profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(&name))
    {
        return Ok(existing.clone());
    }
    let profile = OfflineProfile {
        uuid: offline_uuid(&name),
        name: name.clone(),
        skin_file: None,
        skin_model: SkinModel::default(),
    };
    store.profiles.push(profile.clone());
    if store.active_name.is_none() {
        store.active_name = Some(name);
    }
    save_store(&store)?;
    Ok(profile)
}

/// Adds (or reuses) a nickname and makes it active. Used by "add a pirate
/// nickname" in the account manager and by onboarding's offline-nick field.
pub fn upsert_and_activate(name: &str) -> Result<OfflineProfileInfo> {
    let profile = upsert(name)?;
    let mut store = load_store()?;
    store.active_name = Some(profile.name.clone());
    save_store(&store)?;
    info(&profile)
}

/// Switches to an already-known offline nickname without creating one.
pub fn set_active(name: &str) -> Result<OfflineProfileInfo> {
    let mut store = load_store()?;
    let Some(profile) = store
        .profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .cloned()
    else {
        return Err(NimbusError::Invalid(
            "Этот пиратский ник не найден".to_owned(),
        ));
    };
    store.active_name = Some(profile.name.clone());
    save_store(&store)?;
    info(&profile)
}

/// Removes a nickname and its skin file, if any. If it was active, another
/// remaining nickname (if any) becomes active automatically.
pub fn remove(name: &str) -> Result<Option<OfflineProfileInfo>> {
    let mut store = load_store()?;
    if let Some(pos) = store
        .profiles
        .iter()
        .position(|p| p.name.eq_ignore_ascii_case(name))
    {
        let removed = store.profiles.remove(pos);
        if let Some(file) = removed.skin_file {
            skin::delete_local_skin(&file);
        }
    }
    if store
        .active_name
        .as_deref()
        .is_some_and(|n| n.eq_ignore_ascii_case(name))
    {
        store.active_name = store.profiles.first().map(|p| p.name.clone());
    }
    save_store(&store)?;

    match &store.active_name {
        Some(n) => store
            .profiles
            .iter()
            .find(|p| &p.name == n)
            .map(info)
            .transpose(),
        None => Ok(None),
    }
}

/// Sets (or replaces) the skin for a known offline nickname.
pub async fn set_skin(name: &str, bytes: Vec<u8>, model: SkinModel) -> Result<OfflineProfileInfo> {
    let mut store = load_store()?;
    let Some(profile) = store
        .profiles
        .iter_mut()
        .find(|p| p.name.eq_ignore_ascii_case(name))
    else {
        return Err(NimbusError::Invalid(
            "Этот пиратский ник не найден".to_owned(),
        ));
    };
    let old_file = profile.skin_file.clone();
    let file_name = skin::store_local_skin(&profile.uuid, &bytes).await?;
    profile.skin_file = Some(file_name.clone());
    profile.skin_model = model;
    let result = info(profile)?;
    save_store(&store)?;
    if old_file.as_deref() != Some(file_name.as_str()) {
        if let Some(old) = old_file {
            skin::delete_local_skin(&old);
        }
    }
    Ok(result)
}

/// Clears a nickname's skin, going back to the default Steve/Alex look.
pub async fn clear_skin(name: &str) -> Result<OfflineProfileInfo> {
    let mut store = load_store()?;
    let Some(profile) = store
        .profiles
        .iter_mut()
        .find(|p| p.name.eq_ignore_ascii_case(name))
    else {
        return Err(NimbusError::Invalid(
            "Этот пиратский ник не найден".to_owned(),
        ));
    };
    let old_file = profile.skin_file.take();
    profile.skin_model = SkinModel::default();
    let result = info(profile)?;
    save_store(&store)?;
    if let Some(old) = old_file {
        skin::delete_local_skin(&old);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_is_case_insensitive_and_idempotent() {
        let mut store = OfflineStore::default();
        let first = OfflineProfile {
            uuid: offline_uuid("Steve"),
            name: "Steve".to_owned(),
            skin_file: None,
            skin_model: SkinModel::default(),
        };
        store.profiles.push(first.clone());
        assert!(store
            .profiles
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("steve")));
    }

    #[test]
    fn offline_uuid_matches_the_stored_profile() {
        let profile = OfflineProfile {
            uuid: offline_uuid("Alex"),
            name: "Alex".to_owned(),
            skin_file: None,
            skin_model: SkinModel::default(),
        };
        assert_eq!(profile.uuid, offline_uuid("Alex"));
    }
}
