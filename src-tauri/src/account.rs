//! Persistence for signed-in Microsoft accounts.
//!
//! Kept out of `config.json` on purpose: the config is handed to the WebView on
//! every boot, and tokens have no business being there. This file holds the
//! secrets, and only sanitised [`AccountInfo`] values ever cross the IPC
//! boundary.
//!
//! Nimbus supports multiple signed-in accounts ("profiles"): every account the
//! user has signed into stays in `accounts.json`, and one of them is marked
//! active. Launching always uses the active account; switching between
//! already-signed-in accounts is instant since nothing needs to be
//! re-authenticated.
//!
//! The tokens themselves are encrypted at rest with Windows DPAPI
//! (`crate::winprotect`), the same mechanism Chrome/Edge use for saved
//! passwords: the ciphertext can only be decrypted by the same Windows user
//! account, on the same machine, so a copied `accounts.json` is useless on its
//! own. If DPAPI is ever unavailable, saving falls back to plain text rather
//! than refusing to let the user sign in, and that fallback is logged so it
//! is never a silent downgrade.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::auth::{self, AuthenticatedAccount};
use crate::config::{self, write_atomic};
use crate::error::{NimbusError, Result};
use crate::paths;
use crate::winprotect;

/// The decrypted account shape. Never written to disk directly -- always
/// wrapped in an [`AccountsEnvelope`] first.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAccount {
    pub uuid: String,
    pub name: String,
    /// Xbox user id. Defaulted, so accounts saved before it was stored keep
    /// loading instead of forcing the user to sign in again.
    #[serde(default)]
    pub xuid: String,
    pub mc_access_token: String,
    pub mc_expires_at: u64,
    pub ms_refresh_token: String,
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
            xuid: value.xuid,
            mc_access_token: value.mc_access_token,
            mc_expires_at: value.mc_expires_at,
            ms_refresh_token: value.ms_refresh_token,
        }
    }
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

/// Every signed-in account, plus which one is active. This whole structure is
/// what gets encrypted as a single DPAPI blob -- simpler than per-account
/// envelopes and no weaker, since every account here lives on the same
/// machine/Windows profile anyway.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AccountsStore {
    accounts: Vec<StoredAccount>,
    active_uuid: Option<String>,
}

impl AccountsStore {
    fn active(&self) -> Option<&StoredAccount> {
        let uuid = self.active_uuid.as_ref()?;
        self.accounts.iter().find(|a| &a.uuid == uuid)
    }

    /// Adds a new account, or replaces an existing one with the same uuid
    /// (e.g. a refreshed token), then makes it the active account.
    fn upsert_and_activate(&mut self, account: StoredAccount) {
        let uuid = account.uuid.clone();
        if let Some(existing) = self.accounts.iter_mut().find(|a| a.uuid == uuid) {
            *existing = account;
        } else {
            self.accounts.push(account);
        }
        self.active_uuid = Some(uuid);
    }

    /// Removes an account. If it was active, another remaining account (if
    /// any) becomes active, so the launcher never ends up with accounts on
    /// file but none selected.
    fn remove(&mut self, uuid: &str) {
        self.accounts.retain(|a| a.uuid != uuid);
        if self.active_uuid.as_deref() == Some(uuid) {
            self.active_uuid = self.accounts.first().map(|a| a.uuid.clone());
        }
    }
}

/// What actually gets written to `accounts.json`.
///
/// `protected: true` means `payload` is DPAPI ciphertext of the serialised
/// [`AccountsStore`]; `protected: false` means `payload` is the plain
/// serialised bytes (DPAPI was unavailable when this was saved).
#[derive(Debug, Serialize, Deserialize)]
struct AccountsEnvelope {
    protected: bool,
    payload: Vec<u8>,
}

fn store_file() -> Result<PathBuf> {
    Ok(paths::root()?.join("accounts.json"))
}

/// Path of the pre-multi-account single-account file, kept only so it can be
/// migrated once.
fn legacy_account_file() -> Result<PathBuf> {
    Ok(paths::root()?.join("account.json"))
}

fn decrypt_envelope(raw: &str) -> Option<Vec<u8>> {
    let envelope = serde_json::from_str::<AccountsEnvelope>(raw).ok()?;
    if envelope.protected {
        match winprotect::unprotect(&envelope.payload) {
            Some(bytes) => Some(bytes),
            None => {
                crate::nlog!(
                    "accounts: DPAPI decrypt failed (different machine/user profile, or DPAPI unavailable) -- treating as signed out"
                );
                None
            }
        }
    } else {
        Some(envelope.payload)
    }
}

fn encrypt_and_write(path: &Path, plaintext: &[u8]) -> Result<()> {
    let envelope = match winprotect::protect(plaintext) {
        Some(ciphertext) => AccountsEnvelope {
            protected: true,
            payload: ciphertext,
        },
        None => {
            crate::nlog!(
                "accounts: DPAPI encryption unavailable, storing session tokens in plain text"
            );
            AccountsEnvelope {
                protected: false,
                payload: plaintext.to_vec(),
            }
        }
    };
    let body = serde_json::to_vec_pretty(&envelope)?;
    write_atomic(path, &body)
}

/// One-time migration from the pre-multi-account `account.json` into the new
/// store. Never errors: a migration that cannot be completed just means the
/// user signs in again, which is always an option, so every failure path
/// falls back to an empty store rather than surfacing an error at startup.
fn migrate_legacy() -> Result<AccountsStore> {
    let legacy_path = legacy_account_file()?;
    if !legacy_path.exists() {
        return Ok(AccountsStore::default());
    }
    let Ok(raw) = std::fs::read_to_string(&legacy_path) else {
        return Ok(AccountsStore::default());
    };

    #[derive(Deserialize)]
    struct LegacyEnvelope {
        protected: bool,
        payload: Vec<u8>,
    }

    let single: Option<StoredAccount> = if let Ok(envelope) =
        serde_json::from_str::<LegacyEnvelope>(&raw)
    {
        let plaintext = if envelope.protected {
            winprotect::unprotect(&envelope.payload)
        } else {
            Some(envelope.payload)
        };
        plaintext.and_then(|p| serde_json::from_slice(&p).ok())
    } else {
        serde_json::from_str(&raw).ok()
    };

    let mut store = AccountsStore::default();
    if let Some(account) = single {
        store.upsert_and_activate(account);
        save_store(&store)?;
        // Kept as a .bak instead of deleted outright, in case the new store
        // somehow failed to write correctly.
        let _ = std::fs::rename(&legacy_path, legacy_path.with_extension("json.bak"));
    }
    Ok(store)
}

fn load_store() -> Result<AccountsStore> {
    let path = store_file()?;
    if !path.exists() {
        return migrate_legacy();
    }
    let raw = std::fs::read_to_string(&path)?;
    match decrypt_envelope(&raw) {
        Some(plaintext) => Ok(serde_json::from_slice(&plaintext).unwrap_or_default()),
        None => Ok(AccountsStore::default()),
    }
}

fn save_store(store: &AccountsStore) -> Result<()> {
    let path = store_file()?;
    let plaintext = serde_json::to_vec(store)?;
    encrypt_and_write(&path, &plaintext)
}

/// Every signed-in account, active one first.
pub fn list() -> Result<Vec<AccountInfo>> {
    let store = load_store()?;
    let mut infos: Vec<AccountInfo> = store.accounts.iter().map(|a| a.info()).collect();
    if let Some(active) = &store.active_uuid {
        infos.sort_by_key(|a| if &a.uuid == active { 0 } else { 1 });
    }
    Ok(infos)
}

/// Reads the active account, or `None` when nobody is signed in.
///
/// A corrupt file, an undecryptable envelope (different machine/Windows
/// profile), or a missing file are all treated as "not signed in" rather than
/// an error: the user can always sign in again, and refusing to start would
/// be worse.
pub fn load() -> Result<Option<StoredAccount>> {
    Ok(load_store()?.active().cloned())
}

/// Adds (or updates, if already known) an account and makes it the active
/// one. Used right after a successful sign-in -- signing in again while
/// already signed in adds another account instead of replacing the current
/// one.
pub fn upsert_and_activate(account: StoredAccount) -> Result<AccountInfo> {
    let mut store = load_store()?;
    store.upsert_and_activate(account);
    save_store(&store)?;
    Ok(store
        .active()
        .expect("an account was just activated")
        .info())
}

/// Switches the active account to an already signed-in one.
pub fn set_active(uuid: &str) -> Result<AccountInfo> {
    let mut store = load_store()?;
    if !store.accounts.iter().any(|a| a.uuid == uuid) {
        return Err(NimbusError::Invalid(
            "Этот аккаунт не найден среди сохранённых".to_owned(),
        ));
    }
    store.active_uuid = Some(uuid.to_owned());
    save_store(&store)?;
    Ok(store.active().expect("just set as active").info())
}

/// Removes a stored account. Returns the new active account, if any remain.
pub fn remove(uuid: &str) -> Result<Option<AccountInfo>> {
    let mut store = load_store()?;
    store.remove(uuid);
    save_store(&store)?;
    Ok(store.active().map(|a| a.info()))
}

/// Signs out completely: removes every stored account.
pub fn clear() -> Result<()> {
    save_store(&AccountsStore::default())
}

/// Persists a refreshed token for an account in place, without changing which
/// account is active.
fn save_refreshed(refreshed: StoredAccount) -> Result<()> {
    let mut store = load_store()?;
    let uuid = refreshed.uuid.clone();
    if let Some(existing) = store.accounts.iter_mut().find(|a| a.uuid == uuid) {
        *existing = refreshed;
    } else {
        store.accounts.push(refreshed);
    }
    save_store(&store)
}

/// Returns a usable *active* account, refreshing its Minecraft token when it
/// is stale.
///
/// Called right before launching. If the refresh token itself has been
/// revoked, only that one account is dropped (not every signed-in account),
/// so the UI reflects that a new sign-in is needed instead of failing on
/// every launch or logging every account out.
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
            // Only the Minecraft session needs renewing here — entitlement
            // ownership and the profile cannot have changed since the last
            // full sign-in, so re-checking them (finish_login's other 2
            // requests) on every launch would just re-confirm the same answer.
            let (mc_access_token, mc_expires_at, xuid) =
                auth::refresh_minecraft_session(&tokens.access_token).await?;
            let refreshed = StoredAccount {
                uuid: account.uuid,
                name: account.name,
                // A refresh that somehow reports no xid keeps whatever was
                // stored, rather than clearing a working value.
                xuid: if xuid.is_empty() { account.xuid } else { xuid },
                mc_access_token,
                mc_expires_at,
                ms_refresh_token: tokens.refresh_token,
            };
            save_refreshed(refreshed.clone())?;
            Ok(Some(refreshed))
        }
        Err(err) => {
            // Revoked or expired beyond recovery: drop just this account (not
            // every signed-in account) and report cleanly.
            let mut store = load_store()?;
            store.remove(&account.uuid);
            save_store(&store)?;
            Err(NimbusError::Invalid(format!(
                "Сессия Microsoft истекла, войдите заново ({err})"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(uuid: &str, name: &str) -> StoredAccount {
        StoredAccount {
            uuid: uuid.to_owned(),
            name: name.to_owned(),
            xuid: "2535000000000000".to_owned(),
            mc_access_token: "access-token".to_owned(),
            mc_expires_at: 1_700_000_000,
            ms_refresh_token: "refresh-token".to_owned(),
        }
    }

    #[test]
    fn accounts_saved_before_the_xuid_still_load() {
        let raw = r#"{"uuid":"a","name":"Steve","mcAccessToken":"t","mcExpiresAt":1,"msRefreshToken":"r"}"#;
        let restored: StoredAccount = serde_json::from_str(raw).unwrap();
        assert!(restored.xuid.is_empty());
        assert_eq!(restored.name, "Steve");
    }

    #[test]
    fn upsert_and_activate_adds_and_activates() {
        let mut store = AccountsStore::default();
        store.upsert_and_activate(sample("a", "Steve"));
        assert_eq!(store.active().unwrap().name, "Steve");
        store.upsert_and_activate(sample("b", "Alex"));
        assert_eq!(store.active().unwrap().name, "Alex");
        assert_eq!(store.accounts.len(), 2);
    }

    #[test]
    fn upsert_replaces_existing_uuid_instead_of_duplicating() {
        let mut store = AccountsStore::default();
        store.upsert_and_activate(sample("a", "Steve"));
        let mut renamed = sample("a", "SteveRenamed");
        renamed.mc_access_token = "new-token".to_owned();
        store.upsert_and_activate(renamed);
        assert_eq!(store.accounts.len(), 1);
        assert_eq!(store.active().unwrap().name, "SteveRenamed");
    }

    #[test]
    fn removing_the_active_account_falls_back_to_another() {
        let mut store = AccountsStore::default();
        store.upsert_and_activate(sample("a", "Steve"));
        store.upsert_and_activate(sample("b", "Alex"));
        store.active_uuid = Some("b".to_owned());
        store.remove("b");
        assert_eq!(store.active_uuid.as_deref(), Some("a"));
        assert_eq!(store.accounts.len(), 1);
    }

    #[test]
    fn removing_the_only_account_leaves_nobody_active() {
        let mut store = AccountsStore::default();
        store.upsert_and_activate(sample("a", "Steve"));
        store.remove("a");
        assert!(store.active_uuid.is_none());
        assert!(store.accounts.is_empty());
    }

    #[test]
    fn envelope_roundtrips_through_serde_regardless_of_protection() {
        // Does not touch the real DPAPI call (that needs a real Windows user
        // profile and is covered separately in `winprotect`); this only
        // proves the envelope + AccountsStore serde shapes agree with each
        // other, which is what `load_store`/`save_store` rely on.
        let mut store = AccountsStore::default();
        store.upsert_and_activate(sample("a", "Steve"));
        let plaintext = serde_json::to_vec(&store).unwrap();

        let envelope = AccountsEnvelope {
            protected: false,
            payload: plaintext,
        };
        let raw = serde_json::to_string(&envelope).unwrap();

        let parsed: AccountsEnvelope = serde_json::from_str(&raw).unwrap();
        assert!(!parsed.protected);
        let restored: AccountsStore = serde_json::from_slice(&parsed.payload).unwrap();
        assert_eq!(restored.accounts.len(), 1);
        assert_eq!(restored.active_uuid.as_deref(), Some("a"));
    }

    #[test]
    fn legacy_single_account_shape_still_parses_without_a_store_envelope() {
        let account = sample("a", "Steve");
        let raw = serde_json::to_string(&account).unwrap();
        // Simulates the pre-multi-account on-disk format: a plain
        // StoredAccount, no store wrapper. `migrate_legacy` falls back to
        // this shape (via its own envelope check) when reading account.json.
        assert!(serde_json::from_str::<AccountsEnvelope>(&raw).is_err());
        let restored: StoredAccount = serde_json::from_str(&raw).unwrap();
        assert_eq!(restored.name, "Steve");
    }
}
