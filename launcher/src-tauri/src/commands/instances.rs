use glint_core::{
    Error,
    instance::{self, Instance, NewInstance, Preset},
};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::{CmdResult, ensure_idle};
use crate::state::AppState;

#[tauri::command]
pub fn list_presets() -> Vec<Preset> {
    instance::presets()
}

#[tauri::command]
pub async fn list_instances(state: State<'_, AppState>) -> CmdResult<Vec<Instance>> {
    instance::list(&state.paths).await
}

#[tauri::command]
pub async fn create_instance(state: State<'_, AppState>, new: NewInstance) -> CmdResult<Instance> {
    instance::create(&state.paths, new, state.total_memory_mb).await
}

#[tauri::command]
pub async fn update_instance(
    state: State<'_, AppState>,
    instance: Instance,
) -> CmdResult<Instance> {
    // Only settings are editable; identity and history come from what's on disk.
    let existing = instance::get(&state.paths, &instance.id).await?;
    let updated = Instance {
        id: existing.id,
        created_at: existing.created_at,
        last_played: existing.last_played,
        ..instance
    };
    instance::save(&state.paths, &updated).await?;
    Ok(updated)
}

#[tauri::command]
pub async fn delete_instance(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    ensure_idle(&state, &id, "delete")?;
    instance::delete(&state.paths, &id).await?;
    state.logs.lock().unwrap().remove(&id);
    Ok(())
}

#[tauri::command]
pub async fn duplicate_instance(state: State<'_, AppState>, id: String) -> CmdResult<Instance> {
    // Copying a world while the game is writing to it can produce a corrupt copy.
    ensure_idle(&state, &id, "duplicate")?;
    instance::duplicate(&state.paths, &id).await
}

#[tauri::command]
pub async fn open_instance_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    let instance = instance::get(&state.paths, &id).await?;
    let dir = state.paths.instance_game_dir(&instance.id);
    tokio::fs::create_dir_all(&dir).await?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::Other(format!("couldn't open the folder: {e}")))
}
