//! Nimbus Client library root.
//!
//! This file only declares the modules and wires the Tauri application
//! together; every command body lives under `commands/`.

pub mod account;
pub mod assets;
pub mod auth;
mod config;
pub mod download;
pub mod error;
mod forge_install;
pub mod instance;
pub mod java;
pub mod launcher;
pub mod libraries;
pub mod loader;
pub mod modrinth;
pub mod natives;
pub mod paths;
pub mod presence;
pub mod version;
pub mod webview2;

pub mod commands;

use tauri::Manager;

pub use commands::{InstallCancel, LoginState, RunningGames};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(RunningGames::new())
        .manage(InstallCancel::default())
        .manage(LoginState::default())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // config
            commands::config_cmds::bootstrap,
            commands::config_cmds::set_theme,
            commands::config_cmds::set_offline_username,
            commands::config_cmds::complete_onboarding,
            commands::config_cmds::update_config,
            // Microsoft account
            commands::auth_cmds::set_azure_client_id,
            commands::auth_cmds::begin_ms_login,
            commands::auth_cmds::complete_ms_login,
            commands::auth_cmds::cancel_ms_login,
            commands::auth_cmds::get_account,
            commands::auth_cmds::sign_out,
            // Prism / MultiMC import
            commands::prism::scan_prism_instances,
            commands::prism::import_prism_instance,
            // instances
            commands::instances::list_instances,
            commands::instances::list_versions,
            commands::instances::list_loader_versions,
            commands::instances::delete_instance,
            commands::instances::duplicate_instance,
            commands::instances::rename_instance,
            commands::instances::instance_size,
            commands::instances::set_instance_settings,
            // install
            commands::install::install_version,
            commands::install::install_loader,
            commands::install::cancel_install,
            commands::install::verify_instance,
            // modpack import
            commands::modpack::import_modpack,
            // backup export/import
            commands::backup::export_instance,
            commands::backup::import_instance,
            // launch
            commands::launch::launch_instance,
            commands::launch::kill_instance,
            commands::launch::resolve_java,
            // mods
            commands::mods::list_mods,
            commands::mods::add_mod,
            commands::mods::remove_mod,
            commands::mods::set_mod_enabled,
            // modrinth
            commands::modrinth_cmds::modrinth_search,
            commands::modrinth_cmds::modrinth_versions,
            commands::modrinth_cmds::modrinth_install,
            // files
            commands::files::open_game_dir,
            commands::files::open_mods_dir,
            commands::files::open_screenshots_dir,
            commands::files::open_crash_reports_dir,
            commands::files::open_logs_dir,
            commands::files::list_crash_reports,
            commands::files::read_crash_report,
            commands::files::save_text_file,
            commands::files::cleanup_shared,
            // logs
            commands::logs::get_game_log,
            commands::logs::game_log_path,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Nimbus Client");
}
