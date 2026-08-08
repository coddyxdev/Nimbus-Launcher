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
mod winprotect;

pub mod commands;

use tauri::Manager;

pub use commands::{InstallCancel, LoginState, RunningGames};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch attempt (e.g. double-clicking the shortcut while
            // Nimbus is already running) just focuses the existing window
            // instead of starting a second process that would race the first
            // one over config.json / account.json writes.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
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

            // Discord should show Nimbus Client is open right away, not only
            // once a game is actually running. Rich Presence failures are
            // logged from inside `presence` and never allowed to affect
            // startup, hence the detached task here.
            tauri::async_runtime::spawn(async {
                if config::load().map(|cfg| cfg.discord_rpc).unwrap_or(true) {
                    presence::set_idle().await;
                }
            });

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
            commands::auth_cmds::list_accounts,
            commands::auth_cmds::switch_account,
            commands::auth_cmds::remove_account,
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
            commands::modpack::modrinth_search_modpacks,
            commands::modpack::install_modpack_from_modrinth,
            commands::modpack::check_modpack_update,
            commands::modpack::update_modpack,
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
            // mod updates and dependencies
            commands::mod_updates::check_mod_updates,
            commands::mod_updates::apply_mod_update,
            commands::mod_updates::apply_all_mod_updates,
            commands::mod_updates::mod_dependencies,
            commands::mod_updates::install_mod_with_deps,
            // modrinth
            commands::modrinth_cmds::modrinth_search,
            commands::modrinth_cmds::modrinth_versions,
            commands::modrinth_cmds::modrinth_install,
            commands::modrinth_cmds::modrinth_project,
            commands::modrinth_cmds::modrinth_project_versions,
            // files
            commands::files::open_game_dir,
            commands::files::open_mods_dir,
            commands::files::open_screenshots_dir,
            commands::files::open_crash_reports_dir,
            commands::files::open_logs_dir,
            // modpack export
            commands::export_pack::export_mrpack,
            // screenshot gallery
            commands::gallery::list_screenshots,
            commands::gallery::delete_screenshot,
            commands::gallery::copy_screenshot,
            // restore points
            commands::restore::create_restore_point,
            commands::restore::list_restore_points,
            commands::restore::apply_restore_point,
            commands::restore::delete_restore_point,
            commands::files::list_crash_reports,
            commands::files::read_crash_report,
            commands::files::analyze_crash_report,
            commands::files::save_text_file,
            commands::files::cleanup_shared,
            // logs
            commands::logs::get_game_log,
            commands::logs::game_log_path,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Nimbus Client");
}
