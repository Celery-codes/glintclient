use glint_core::{
    Error,
    auth::{
        self, AuthError,
        msa::{self, DeviceCode},
        store::{Account, Accounts},
    },
    config::Config,
    launch::Session,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::CmdResult;
use crate::state::AppState;

pub const AUTH_FINISHED: &str = "auth-finished";

/// What the sign-in dialog shows. The device code itself stays in the backend.
#[derive(Serialize)]
pub struct LoginPrompt {
    user_code: String,
    verification_uri: String,
    expires_in: u64,
}

#[derive(Clone, Serialize)]
struct AuthFinished {
    account: Option<Account>,
    error: Option<serde_json::Value>,
}

async fn client_id(state: &AppState) -> CmdResult<String> {
    Ok(Config::load(&state.paths)
        .await?
        .azure_client_id()
        .ok_or(AuthError::MissingClientId)?
        .value)
}

/// A valid Minecraft session for the active account, refreshed silently if needed.
pub async fn active_session(state: &AppState) -> CmdResult<Session> {
    let accounts = Accounts::load(&state.paths).await?;
    let account = accounts.active().ok_or(AuthError::NotSignedIn)?;
    if let Some(signed_in) = state.sessions.lock().unwrap().get(&account.uuid)
        && signed_in.is_fresh()
    {
        return Ok(signed_in.session.clone());
    }
    let client_id = client_id(state).await?;
    let signed_in = auth::refresh(
        &state.client,
        &state.paths,
        &state.secrets,
        &client_id,
        &account.uuid,
    )
    .await?;
    let session = signed_in.session.clone();
    state
        .sessions
        .lock()
        .unwrap()
        .insert(account.uuid.clone(), signed_in);
    Ok(session)
}

/// Start a device-code sign-in. Returns the code to show; the result arrives later as
/// an `auth-finished` event once the user has entered it in their browser.
#[tauri::command]
pub async fn auth_begin(app: AppHandle, state: State<'_, AppState>) -> CmdResult<LoginPrompt> {
    let client_id = client_id(&state).await?;
    let code = msa::request_device_code(&state.client, &client_id).await?;
    let prompt = LoginPrompt {
        user_code: code.user_code.clone(),
        verification_uri: code.verification_uri.clone(),
        expires_in: code.expires_in,
    };
    let task = tokio::spawn(finish_login(app.clone(), client_id, code));
    let previous = state
        .pending_login
        .lock()
        .unwrap()
        .replace(task.abort_handle());
    if let Some(previous) = previous {
        previous.abort();
    }
    Ok(prompt)
}

async fn finish_login(app: AppHandle, client_id: String, code: DeviceCode) {
    let state = app.state::<AppState>();
    let result: Result<_, Error> = async {
        let tokens = msa::poll_for_tokens(&state.client, &client_id, &code).await?;
        auth::sign_in(&state.client, &state.paths, &state.secrets, tokens).await
    }
    .await;
    state.pending_login.lock().unwrap().take();
    let payload = match result {
        Ok(signed_in) => {
            let account = signed_in.account.clone();
            state
                .sessions
                .lock()
                .unwrap()
                .insert(account.uuid.clone(), signed_in);
            AuthFinished {
                account: Some(account),
                error: None,
            }
        }
        Err(error) => AuthFinished {
            account: None,
            error: serde_json::to_value(&error).ok(),
        },
    };
    let _ = app.emit(AUTH_FINISHED, payload);
}

#[tauri::command]
pub fn auth_cancel(state: State<'_, AppState>) {
    if let Some(pending) = state.pending_login.lock().unwrap().take() {
        pending.abort();
    }
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> CmdResult<Accounts> {
    Accounts::load(&state.paths).await
}

#[tauri::command]
pub async fn set_active_account(state: State<'_, AppState>, uuid: String) -> CmdResult<Accounts> {
    let mut accounts = Accounts::load(&state.paths).await?;
    if !accounts.set_active(&uuid) {
        return Err(Error::NotFound("account".into()));
    }
    accounts.save(&state.paths).await?;
    Ok(accounts)
}

#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, uuid: String) -> CmdResult<Accounts> {
    state.sessions.lock().unwrap().remove(&uuid);
    auth::sign_out(&state.paths, &state.secrets, &uuid).await
}
