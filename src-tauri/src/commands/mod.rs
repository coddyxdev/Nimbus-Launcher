//! Tauri command surface, split by area.
//!
//! `lib.rs` only wires these together; every command body lives here so no
//! single file grows past a few hundred lines.

pub mod auth_cmds;
pub mod background;
pub mod backup;
pub mod config_cmds;
pub mod export_pack;
pub mod files;
pub mod gallery;
pub mod install;
pub mod instances;
pub mod launch;
pub mod logs;
pub mod mod_updates;
pub mod modpack;
pub mod mods;
pub mod news;

pub mod modrinth_cmds;
pub mod prism;
pub mod restore;
pub mod servers;
pub mod shared;
pub mod sysmem;

pub use auth_cmds::LoginState;
pub use shared::{CancelToken, InstallCancel, RunningGames};
