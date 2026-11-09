use std::collections::HashSet;

use glint_core::meta::{
    fabric::{self, LoaderVersion},
    mojang,
};
use serde::Serialize;
use tauri::State;

use super::CmdResult;
use crate::state::AppState;

#[derive(Serialize)]
pub struct GameVersion {
    id: String,
    /// `release` or `snapshot`.
    kind: String,
    release_time: String,
}

#[derive(Serialize)]
pub struct GameVersions {
    latest_release: String,
    versions: Vec<GameVersion>,
}

/// Minecraft versions an instance can be created for: 1.19 or newer and supported by
/// Fabric. Newest first.
#[tauri::command]
pub async fn list_game_versions(
    state: State<'_, AppState>,
    include_snapshots: bool,
) -> CmdResult<GameVersions> {
    let (manifest, fabric_versions) = tokio::try_join!(
        mojang::version_manifest(&state.client, &state.paths),
        fabric::game_versions(&state.client, &state.paths),
    )?;
    let on_fabric: HashSet<&str> = fabric_versions.iter().map(|v| v.version.as_str()).collect();
    let versions = manifest
        .versions
        .iter()
        .filter(|v| v.is_supported() && on_fabric.contains(v.id.as_str()))
        .filter(|v| include_snapshots || v.kind == "release")
        .map(|v| GameVersion {
            id: v.id.clone(),
            kind: v.kind.clone(),
            release_time: v.release_time.clone(),
        })
        .collect();
    Ok(GameVersions {
        latest_release: manifest.latest.release,
        versions,
    })
}

#[tauri::command]
pub async fn list_loader_versions(
    state: State<'_, AppState>,
    mc_version: String,
) -> CmdResult<Vec<LoaderVersion>> {
    fabric::loader_versions(&state.client, &state.paths, &mc_version).await
}
