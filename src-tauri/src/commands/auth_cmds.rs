//! Microsoft account commands: device-code sign-in, sign-out, multi-account state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::account::{self, AccountInfo};
use crate::auth::{self, DeviceCode};
use crate::config::{self, Config};
use crate::error::{NimbusError, Result};

use super::shared::{lock, open_external_url};

/// Holds the device code between `begin_ms_login` and `complete_ms_login`.
///
/// Sign-in is split in two commands so the UI can render the code immediately
/// and then await the (potentially minutes-long) polling call separately.
#[derive(Default)]
pub struct LoginState {
    pending: Mutex<Option<DeviceCode>>,
    /// Kept apart from `pending` so the sign-in page can still be reopened
    /// after `complete_ms_login` consumed the code.
    verification_uri: Mutex<Option<String>>,
    cancelled: AtomicBool,
}

impl LoginState {
    fn set(&self, device: DeviceCode) {
        self.cancelled.store(false, Ordering::SeqCst);
        *lock(&self.verification_uri) = Some(device.verification_uri.clone());
        *lock(&self.pending) = Some(device);
    }

    fn verification_uri(&self) -> Option<String> {
        lock(&self.verification_uri).clone()
    }

    fn take(&self) -> Option<DeviceCode> {
        lock(&self.pending).take()
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        *lock(&self.pending) = None;
        *lock(&self.verification_uri) = None;
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// The client id sign-in uses: the user's own Azure application when they
/// configured one, otherwise the id built into the launcher.
fn client_id() -> Result<String> {
    let cfg = config::load()?;
    auth::resolve_client_id(cfg.azure_client_id.as_deref())
}

/// Stores the Azure application id used for Microsoft sign-in.
///
/// An empty value clears it, which puts the launcher back into offline-only
/// mode without touching any already signed-in accounts.
#[tauri::command]
pub fn set_azure_client_id(client_id: String) -> Result<Config> {
    config::update(|cfg| {
        let trimmed = client_id.trim();
        cfg.azure_client_id = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        };
        Ok(())
    })
}

/// Starts sign-in and returns the code the user must enter.
#[tauri::command]
pub async fn begin_ms_login(app: AppHandle) -> Result<DeviceCode> {
    let id = client_id()?;
    let device = auth::request_device_code(&id).await?;
    app.state::<LoginState>().set(device.clone());
    Ok(device)
}

/// Waits for the user to finish signing in, then stores the account.
///
/// Long-running by nature: it returns when the browser step completes, the code
/// expires, or `cancel_ms_login` is called.
///
/// If another account is already signed in, this adds the new one alongside
/// it and makes it active, rather than replacing it — that is what lets
/// `list_accounts`/`switch_account` offer more than one profile.
#[tauri::command]
pub async fn complete_ms_login(app: AppHandle) -> Result<AccountInfo> {
    let id = client_id()?;
    let state = app.state::<LoginState>();
    let device = state
        .take()
        .ok_or_else(|| NimbusError::Invalid("Вход не начат".to_owned()))?;

    // The closure is what lets the cancel command interrupt the poll loop.
    let handle = app.clone();
    let cancelled = move || handle.state::<LoginState>().is_cancelled();

    let tokens = auth::await_device_token(&id, &device, &cancelled).await?;
    let account: account::StoredAccount = auth::finish_login(tokens).await?.into();
    account::upsert_and_activate(account)
}

#[tauri::command]
pub fn cancel_ms_login(app: AppHandle) -> Result<()> {
    app.state::<LoginState>().cancel();
    Ok(())
}

/// Opens the Microsoft page for the sign-in currently in progress, so nobody
/// has to retype a URL by hand.
///
/// Only the address Microsoft returned for this sign-in is ever opened -- the
/// frontend cannot pass one in, which keeps this from becoming a way to open
/// arbitrary links from the web view.
#[tauri::command]
pub fn open_login_page(app: AppHandle) -> Result<()> {
    let uri = app
        .state::<LoginState>()
        .verification_uri()
        .ok_or_else(|| NimbusError::Invalid("Вход не начат".to_owned()))?;

    if !uri.starts_with("https://") {
        return Err(NimbusError::Invalid(
            "Microsoft вернул неожидаемый адрес страницы входа".to_owned(),
        ));
    }

    // Goes through the launcher's single hardened opener (scheme check plus
    // argument-breakout check) instead of being a second rundll32 call site.
    open_external_url(&uri)
}

/// The currently active account, or `null` for offline mode.
#[tauri::command]
pub fn get_account() -> Result<Option<AccountInfo>> {
    Ok(account::load()?.map(|a| a.info()))
}

/// Every signed-in account, active one first. The UI uses this to render the
/// account switcher instead of a single "signed in as" row.
#[tauri::command]
pub fn list_accounts() -> Result<Vec<AccountInfo>> {
    account::list()
}

/// Makes an already signed-in account the active one. No network calls are
/// needed since the account's tokens are already on disk.
#[tauri::command]
pub fn switch_account(uuid: String) -> Result<AccountInfo> {
    account::set_active(&uuid)
}

/// Removes one signed-in account. If it was active, another remaining account
/// (if any) becomes active automatically.
#[tauri::command]
pub fn remove_account(uuid: String) -> Result<Option<AccountInfo>> {
    account::remove(&uuid)
}

/// Signs out completely: removes every signed-in account.
#[tauri::command]
pub fn sign_out() -> Result<()> {
    account::clear()
}
