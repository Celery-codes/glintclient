//! Glint launcher core: everything the launcher does that isn't UI.
//!
//! This crate has no Tauri dependency so it can be unit-tested and reused headlessly.

pub mod auth;
pub mod config;
pub mod download;
pub mod error;
pub mod fs_util;
pub mod install;
pub mod instance;
pub mod java;
pub mod launch;
pub mod meta;
pub mod paths;
pub mod system;

pub use error::{Error, Result};

/// Reported to the game as `minecraft.launcher.brand`.
pub const LAUNCHER_NAME: &str = "Glint";
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shared HTTP client. Modrinth and Mojang both ask for an identifying User-Agent.
pub fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(format!(
            "Glint/{LAUNCHER_VERSION} (dev.glintclient.launcher)"
        ))
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()?)
}
