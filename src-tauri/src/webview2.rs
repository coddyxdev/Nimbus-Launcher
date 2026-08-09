//! Startup guard: the UI renders through WebView2, and when the runtime is
//! missing the window silently never appears. Check the registry before
//! creating the window and show a native dialog with the official
//! bootstrapper link instead of an empty process.

#[cfg(windows)]
mod imp {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    /// Official Microsoft Evergreen Bootstrapper link (documented in the
    /// WebView2 distribution guide). Stable across runtime versions.
    const BOOTSTRAPPER_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

    /// The fixed AppId of the WebView2 Evergreen Runtime.
    const WEBVIEW2_APPID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

    /// A registered runtime has a non-empty `pv` that is not the "not
    /// installed" sentinel. Extracted for testability.
    fn pv_indicates_installed(pv: &str) -> bool {
        let pv = pv.trim();
        !pv.is_empty() && pv != "0.0.0.0"
    }

    fn runtime_installed() -> bool {
        let locations = [
            // System-wide install, 64-bit view.
            (
                HKEY_LOCAL_MACHINE,
                format!(
                    r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_APPID}"
                ),
            ),
            // System-wide install, native view.
            (
                HKEY_LOCAL_MACHINE,
                format!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_APPID}"),
            ),
            // Per-user install.
            (
                HKEY_CURRENT_USER,
                format!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_APPID}"),
            ),
        ];

        locations.iter().any(|(hive, path)| {
            RegKey::predef(*hive)
                .open_subkey(path)
                .and_then(|key| key.get_value::<String, _>("pv"))
                .map(|pv| pv_indicates_installed(&pv))
                .unwrap_or(false)
        })
    }

    /// Returns true when the app may proceed. When the runtime is absent,
    /// shows a native dialog, optionally opens the download page, and returns
    /// false so the process exits instead of showing a dead window.
    pub fn ensure_runtime() -> bool {
        if runtime_installed() {
            return true;
        }

        let answer = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Nimbus Client — требуется WebView2")
            .set_description(
                "Для работы лаунчера нужен Microsoft Edge WebView2 Runtime.\n\n\
                 Нажмите «Да», чтобы скачать установщик с сайта Microsoft, \
                 установите его и запустите лаунчер снова.",
            )
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();

        if answer == rfd::MessageDialogResult::Yes {
            // Reuses the launcher's single hardened opener, which also avoids
            // flashing a console window from this GUI-subsystem process.
            let _ = crate::commands::shared::open_external_url(BOOTSTRAPPER_URL);
        }
        false
    }

    #[cfg(test)]
    mod tests {
        use super::pv_indicates_installed;

        #[test]
        fn pv_sentinel_means_absent() {
            assert!(!pv_indicates_installed("0.0.0.0"));
            assert!(!pv_indicates_installed(""));
            assert!(!pv_indicates_installed("   "));
        }

        #[test]
        fn pv_real_version_means_installed() {
            assert!(pv_indicates_installed("131.0.2903.112"));
        }
    }
}

#[cfg(windows)]
pub use imp::ensure_runtime;

/// Non-Windows builds render through system WebKit and need no check.
#[cfg(not(windows))]
pub fn ensure_runtime() -> bool {
    true
}
