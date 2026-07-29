//! Tauri command surface, split by area.
//!
//! `lib.rs` only wires these together; every command body lives here so no
//! single file grows past a few hundred lines.

pub mod auth_cmds;
pub mod backup;
pub mod config_cmds;
pub mod files;
pub mod install;
pub mod instances;
pub mod launch;
pub mod logs;
pub mod modpack;
pub mod mods;
pub mod modrinth_cmds;
pub mod prism;
pub mod shared;

pub use auth_cmds::LoginState;
pub use shared::{InstallCancel, RunningGames};
