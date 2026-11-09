use glint_core::{
    config::{Config, Source},
    system::{self, MemoryLimits},
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::CmdResult;
use crate::state::AppState;

#[derive(Serialize)]
pub struct SystemInfo {
    launcher_version: &'static str,
    memory: MemoryLimits,
    data_dir: String,
    /// Offline launch exists for testing the launch pipeline and is compiled out of
    /// release builds.
    offline_launch: bool,
}

#[tauri::command]
pub fn system_info(state: State<'_, AppState>) -> SystemInfo {
    SystemInfo {
        launcher_version: glint_core::LAUNCHER_VERSION,
        memory: system::memory_limits(state.total_memory_mb),
        data_dir: state.paths.root().to_string_lossy().into_owned(),
        offline_launch: cfg!(debug_assertions),
    }
}

/// The saved values plus where the value actually in effect comes from, so the UI can
/// say "set by environment variable" instead of showing an empty box.
#[derive(Serialize)]
pub struct SettingsView {
    azure_client_id: Option<String>,
    azure_client_id_source: Option<Source>,
    curseforge_api_key: Option<String>,
    curseforge_api_key_source: Option<Source>,
    max_concurrent_downloads: usize,
}

impl From<Config> for SettingsView {
    fn from(config: Config) -> Self {
        Self {
            azure_client_id_source: config.azure_client_id().map(|r| r.source),
            curseforge_api_key_source: config.curseforge_api_key().map(|r| r.source),
            azure_client_id: config.azure_client_id,
            curseforge_api_key: config.curseforge_api_key,
            max_concurrent_downloads: config.max_concurrent_downloads,
        }
    }
}

#[derive(Deserialize)]
pub struct SettingsInput {
    azure_client_id: Option<String>,
    curseforge_api_key: Option<String>,
    max_concurrent_downloads: usize,
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<SettingsView> {
    Ok(Config::load(&state.paths).await?.into())
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: SettingsInput,
) -> CmdResult<SettingsView> {
    let clean = |value: Option<String>| {
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let config = Config {
        azure_client_id: clean(settings.azure_client_id),
        curseforge_api_key: clean(settings.curseforge_api_key),
        max_concurrent_downloads: settings.max_concurrent_downloads.clamp(1, 64),
    };
    config.save(&state.paths).await?;
    Ok(config.into())
}

#[tauri::command]
pub fn open_data_folder(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    app.opener()
        .open_path(state.paths.root().to_string_lossy(), None::<&str>)
        .map_err(|e| glint_core::Error::Other(format!("couldn't open the folder: {e}")))
}
