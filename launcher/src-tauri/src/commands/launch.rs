use std::{collections::HashMap, sync::Arc};

use glint_core::{
    Error,
    config::Config,
    download::{Progress, ProgressFn},
    instance,
    launch::{self, GameEvent, LogLine, Session},
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::{CmdResult, auth};
use crate::state::{Activity, AppState};

const INSTANCE_STATE: &str = "instance-state";
const INSTANCE_PROGRESS: &str = "instance-progress";
const INSTANCE_LOG: &str = "instance-log";

#[derive(Clone, Serialize)]
struct StateEvent<'a> {
    id: &'a str,
    /// `preparing`, `running` or `stopped`.
    state: &'static str,
    exit_code: Option<i32>,
    /// Stopped because the user asked, so a non-zero exit code isn't a crash.
    killed: bool,
    error: Option<serde_json::Value>,
}

impl<'a> StateEvent<'a> {
    fn new(id: &'a str, state: &'static str) -> Self {
        Self {
            id,
            state,
            exit_code: None,
            killed: false,
            error: None,
        }
    }
}

#[derive(Clone, Serialize)]
struct ProgressEvent<'a> {
    id: &'a str,
    #[serde(flatten)]
    progress: Progress,
}

#[derive(Clone, Serialize)]
struct LogEvent<'a> {
    id: &'a str,
    lines: &'a [LogLine],
}

/// Install whatever is missing and start the game. Returns immediately; progress, log
/// output and the outcome arrive as `instance-*` events.
#[tauri::command]
pub async fn launch_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    offline: bool,
) -> CmdResult<()> {
    if offline && !cfg!(debug_assertions) {
        return Err(Error::Launch(
            "Offline launch is only available in development builds.".into(),
        ));
    }
    instance::get(&state.paths, &id).await?;

    let mut activity = state.activity.lock().unwrap();
    if activity.contains_key(&id) {
        return Err(Error::Launch("This instance is already running.".into()));
    }
    let task = tokio::spawn(run(app.clone(), id.clone(), offline));
    let _ = app.emit(INSTANCE_STATE, StateEvent::new(&id, "preparing"));
    activity.insert(id, Activity::Preparing(task.abort_handle()));
    Ok(())
}

async fn run(app: AppHandle, id: String, offline: bool) {
    let state = app.state::<AppState>();
    if let Err(error) = prepare_and_start(&app, &state, &id, offline).await {
        state.activity.lock().unwrap().remove(&id);
        let event = StateEvent {
            error: serde_json::to_value(&error).ok(),
            ..StateEvent::new(&id, "stopped")
        };
        let _ = app.emit(INSTANCE_STATE, event);
    }
}

async fn prepare_and_start(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    offline: bool,
) -> CmdResult<()> {
    let instance = instance::get(&state.paths, id).await?;
    // Sign-in comes first so a missing or expired login fails in a second, not after a
    // long download.
    let session = if offline {
        Session::offline("GlintDev")
    } else {
        auth::active_session(state).await?
    };
    let config = Config::load(&state.paths).await?;

    let progress: ProgressFn = {
        let (app, id) = (app.clone(), id.to_string());
        Arc::new(move |progress: Progress| {
            let _ = app.emit(INSTANCE_PROGRESS, ProgressEvent { id: &id, progress });
        })
    };
    let prepared = launch::prepare(
        &state.client,
        &state.paths,
        &instance,
        config.max_concurrent_downloads,
        &progress,
    )
    .await?;
    let command = launch::command(&state.paths, &instance, &prepared, &session).await?;

    state.logs.lock().unwrap().remove(id);
    let on_event = {
        let (app, id) = (app.clone(), id.to_string());
        move |event: GameEvent| {
            let state = app.state::<AppState>();
            match event {
                GameEvent::Log(lines) => {
                    state.push_log(&id, &lines);
                    let _ = app.emit(
                        INSTANCE_LOG,
                        LogEvent {
                            id: &id,
                            lines: &lines,
                        },
                    );
                }
                GameEvent::Exit { code, killed } => {
                    state.activity.lock().unwrap().remove(&id);
                    let event = StateEvent {
                        exit_code: code,
                        killed,
                        ..StateEvent::new(&id, "stopped")
                    };
                    let _ = app.emit(INSTANCE_STATE, event);
                }
            }
        }
    };
    {
        // Held across the spawn so the exit handler above, which takes the same lock,
        // can't run before the running game has been recorded.
        let mut activity = state.activity.lock().unwrap();
        // Cancelled while this task was between await points: the abort hasn't landed
        // yet, but the entry is gone. Don't start a game the user just cancelled.
        if !matches!(activity.get(id), Some(Activity::Preparing(_))) {
            return Ok(());
        }
        let handle = launch::process::spawn(command, on_event)?;
        activity.insert(id.to_string(), Activity::Running(handle));
    }
    let _ = app.emit(INSTANCE_STATE, StateEvent::new(id, "running"));
    let _ = instance::mark_played(&state.paths, id).await;
    Ok(())
}

/// Cancel an install in progress, or stop the running game.
#[tauri::command]
pub fn stop_instance(app: AppHandle, state: State<'_, AppState>, id: String) {
    let mut activity = state.activity.lock().unwrap();
    match activity.get_mut(&id) {
        Some(Activity::Preparing(task)) => {
            task.abort();
            activity.remove(&id);
            let event = StateEvent {
                killed: true,
                ..StateEvent::new(&id, "stopped")
            };
            let _ = app.emit(INSTANCE_STATE, event);
        }
        // The game's exit event does the cleanup and notifies the UI.
        Some(Activity::Running(game)) => game.kill(),
        None => {}
    }
}

/// Which instances are busy right now, for the UI to load when it starts.
#[tauri::command]
pub fn instance_states(state: State<'_, AppState>) -> HashMap<String, &'static str> {
    state
        .activity
        .lock()
        .unwrap()
        .iter()
        .map(|(id, activity)| {
            let name = match activity {
                Activity::Preparing(_) => "preparing",
                Activity::Running(_) => "running",
            };
            (id.clone(), name)
        })
        .collect()
}

/// The buffered output of the instance's current or most recent run.
#[tauri::command]
pub fn instance_log(state: State<'_, AppState>, id: String) -> Vec<LogLine> {
    state
        .logs
        .lock()
        .unwrap()
        .get(&id)
        .map(|lines| lines.iter().cloned().collect())
        .unwrap_or_default()
}
