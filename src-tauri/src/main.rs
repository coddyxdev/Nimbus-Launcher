// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Guard before any window exists: without the WebView2 runtime Tauri
    // would start a process that shows nothing at all.
    if !nimbus_lib::webview2::ensure_runtime() {
        return;
    }
    nimbus_lib::run()
}
