//! Microsoft account commands: device-code sign-in, sign-out, account state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::account::{self, AccountInfo};
use crate::auth::{self, DeviceCode};
use crate::config::{self, Config};
use crate::error::{NimbusError, Result};

use super::shared::lock;

/// Holds the device code between `begin_ms_login` and `complete_ms_login`.
///
/// Sign-in is split in two commands so the UI can render the code immediately
/// and then await the (potentially minutes-long) polling call separately.
#[derive(Default)]
pub struct LoginState {
    pending: Mutex<Option<DeviceCode>>,
    cancelled: AtomicBool,
}

impl LoginState {
    fn set(&self, device: DeviceCode) {
        self.cancelled.store(false, Ordering::SeqCst);
        *lock(&self.pending) = Some(device);
    }

    fn take(&self) -> Option<DeviceCode> {
        lock(&self.pending).take()
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        *lock(&self.pending) = None;
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

fn client_id() -> Result<String> {
    let cfg = config::load()?;
    let id = cfg.azure_client_id.unwrap_or_default().trim().to_owned();
    if id.is_empty() {
        return Err(NimbusError::Invalid(
            "Не указан Azure Client ID — задайте его в настройках".to_owned(),
        ));
    }
    Ok(id)
}

/// Stores the Azure application id used for Microsoft sign-in.
///
/// An empty value clears it, which puts the launcher back into offline-only
/// mode without touching an already signed-in account.
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
    account::save(&account)?;
    Ok(account.info())
}

#[tauri::command]
pub fn cancel_ms_login(app: AppHandle) -> Result<()> {
    app.state::<LoginState>().cancel();
    Ok(())
}

/// The currently signed-in account, or `null` for offline mode.
#[tauri::command]
pub fn get_account() -> Result<Option<AccountInfo>> {
    Ok(account::load()?.map(|a| a.info()))
}

#[tauri::command]
pub fn sign_out() -> Result<()> {
    account::clear()
}
