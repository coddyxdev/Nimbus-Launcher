//! Persistence for signed-in Ely.by accounts.
//!
//! Structured exactly like `account.rs`'s Microsoft accounts -- several
//! signed-in accounts, one active, tokens encrypted at rest with Windows
//! DPAPI -- see that file's module docs for the full reasoning. Kept as a
//! separate store (`ely_accounts.json`) rather than merged into the
//! Microsoft one so the two token formats never have to share a schema.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::write_atomic;
use crate::ely;
use crate::error::{NimbusError, Result};
use crate::paths;
use crate::winprotect;

/// The decrypted account shape. Never written to disk directly -- always
/// wrapped in an [`ElyEnvelope`] first.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredElyAccount {
    pub uuid: String,
    pub name: String,
    pub access_token: String,
    pub client_token: String,
}

impl StoredElyAccount {
    pub fn info(&self) -> ElyAccountInfo {
        ElyAccountInfo {
            uuid: self.uuid.clone(),
            name: self.name.clone(),
        }
    }
}

/// What the UI is allowed to see.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElyAccountInfo {
    pub uuid: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ElyStore {
    accounts: Vec<StoredElyAccount>,
    active_uuid: Option<String>,
}

impl ElyStore {
    fn active(&self) -> Option<&StoredElyAccount> {
        let uuid = self.active_uuid.as_ref()?;
        self.accounts.iter().find(|a| &a.uuid == uuid)
    }

    fn upsert_and_activate(&mut self, account: StoredElyAccount) {
        let uuid = account.uuid.clone();
        if let Some(existing) = self.accounts.iter_mut().find(|a| a.uuid == uuid) {
            *existing = account;
        } else {
            self.accounts.push(account);
        }
        self.active_uuid = Some(uuid);
    }

    fn remove(&mut self, uuid: &str) {
        self.accounts.retain(|a| a.uuid != uuid);
        if self.active_uuid.as_deref() == Some(uuid) {
            self.active_uuid = self.accounts.first().map(|a| a.uuid.clone());
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ElyEnvelope {
    protected: bool,
    payload: Vec<u8>,
}

fn store_file() -> Result<PathBuf> {
    Ok(paths::root()?.join("ely_accounts.json"))
}

fn decrypt_envelope(raw: &str) -> Option<Vec<u8>> {
    let envelope = serde_json::from_str::<ElyEnvelope>(raw).ok()?;
    if envelope.protected {
        winprotect::unprotect(&envelope.payload)
    } else {
        Some(envelope.payload)
    }
}

fn encrypt_and_write(path: &Path, plaintext: &[u8]) -> Result<()> {
    let envelope = match winprotect::protect(plaintext) {
        Some(ciphertext) => ElyEnvelope {
            protected: true,
            payload: ciphertext,
        },
        None => ElyEnvelope {
            protected: false,
            payload: plaintext.to_vec(),
        },
    };
    write_atomic(path, serde_json::to_vec_pretty(&envelope)?.as_slice())
}

fn load_store() -> Result<ElyStore> {
    let path = store_file()?;
    if !path.exists() {
        return Ok(ElyStore::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    match decrypt_envelope(&raw) {
        Some(plaintext) => Ok(serde_json::from_slice(&plaintext).unwrap_or_default()),
        None => Ok(ElyStore::default()),
    }
}

fn save_store(store: &ElyStore) -> Result<()> {
    let path = store_file()?;
    let plaintext = serde_json::to_vec(store)?;
    encrypt_and_write(&path, &plaintext)
}

/// Every signed-in Ely.by account, active one first.
pub fn list() -> Result<Vec<ElyAccountInfo>> {
    let store = load_store()?;
    let mut infos: Vec<ElyAccountInfo> = store.accounts.iter().map(|a| a.info()).collect();
    if let Some(active) = &store.active_uuid {
        infos.sort_by_key(|a| if &a.uuid == active { 0 } else { 1 });
    }
    Ok(infos)
}

/// Reads the active Ely.by account, or `None` when nobody is signed in.
pub fn load() -> Result<Option<StoredElyAccount>> {
    Ok(load_store()?.active().cloned())
}

/// Adds (or updates) an account and makes it active.
pub fn upsert_and_activate(account: StoredElyAccount) -> Result<ElyAccountInfo> {
    let mut store = load_store()?;
    store.upsert_and_activate(account);
    save_store(&store)?;
    Ok(store
        .active()
        .expect("an account was just activated")
        .info())
}

/// Switches the active account to an already signed-in one.
pub fn set_active(uuid: &str) -> Result<ElyAccountInfo> {
    let mut store = load_store()?;
    if !store.accounts.iter().any(|a| a.uuid == uuid) {
        return Err(NimbusError::Invalid(
            "Этот аккаунт Ely.by не найден среди сохранённых".to_owned(),
        ));
    }
    store.active_uuid = Some(uuid.to_owned());
    save_store(&store)?;
    Ok(store.active().expect("just set as active").info())
}

/// Removes a stored account. Returns the new active account, if any remain.
pub fn remove(uuid: &str) -> Result<Option<ElyAccountInfo>> {
    let mut store = load_store()?;
    store.remove(uuid);
    save_store(&store)?;
    Ok(store.active().map(|a| a.info()))
}

/// Signs out completely: removes every signed-in Ely.by account.
pub fn clear() -> Result<()> {
    save_store(&ElyStore::default())
}

fn save_refreshed(refreshed: StoredElyAccount) -> Result<()> {
    let mut store = load_store()?;
    let uuid = refreshed.uuid.clone();
    if let Some(existing) = store.accounts.iter_mut().find(|a| a.uuid == uuid) {
        *existing = refreshed;
    } else {
        store.accounts.push(refreshed);
    }
    save_store(&store)
}

/// Returns a usable active Ely.by account, refreshing its session first.
///
/// Unlike Microsoft tokens, Ely.by's Yggdrasil tokens carry no expiry the
/// client can check locally, so every launch simply asks for a fresh one
/// instead of guessing when the old one might still work. If the refresh
/// itself has been revoked, only that one account is dropped, matching
/// `account::valid_account`'s handling for Microsoft.
pub async fn valid_account() -> Result<Option<StoredElyAccount>> {
    let Some(account) = load()? else {
        return Ok(None);
    };

    match ely::refresh(&account.access_token, &account.client_token).await {
        Ok(tokens) => {
            let refreshed = StoredElyAccount {
                uuid: account.uuid,
                name: tokens.name,
                access_token: tokens.access_token,
                client_token: tokens.client_token,
            };
            save_refreshed(refreshed.clone())?;
            Ok(Some(refreshed))
        }
        Err(err) => {
            // A network hiccup or a 5xx from Ely.by is not proof the session
            // is bad -- keep the account and let the next launch retry.
            if err.is_retriable() {
                return Err(NimbusError::Invalid(format!(
                    "Не удалось обновить сессию Ely.by — проверьте подключение к интернету и попробуйте снова ({err})"
                )));
            }
            let mut store = load_store()?;
            store.remove(&account.uuid);
            save_store(&store)?;
            Err(NimbusError::Invalid(format!(
                "Сессия Ely.by истекла, войдите заново ({err})"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(uuid: &str, name: &str) -> StoredElyAccount {
        StoredElyAccount {
            uuid: uuid.to_owned(),
            name: name.to_owned(),
            access_token: "token".to_owned(),
            client_token: "client".to_owned(),
        }
    }

    #[test]
    fn upsert_and_activate_adds_and_activates() {
        let mut store = ElyStore::default();
        store.upsert_and_activate(sample("a", "Steve"));
        assert_eq!(store.active().unwrap().name, "Steve");
        store.upsert_and_activate(sample("b", "Alex"));
        assert_eq!(store.active().unwrap().name, "Alex");
        assert_eq!(store.accounts.len(), 2);
    }

    #[test]
    fn removing_the_active_account_falls_back_to_another() {
        let mut store = ElyStore::default();
        store.upsert_and_activate(sample("a", "Steve"));
        store.upsert_and_activate(sample("b", "Alex"));
        store.remove("b");
        assert_eq!(store.active().unwrap().name, "Steve");
    }

    #[test]
    fn removing_the_only_account_leaves_nobody_active() {
        let mut store = ElyStore::default();
        store.upsert_and_activate(sample("a", "Steve"));
        store.remove("a");
        assert!(store.active().is_none());
    }
}
