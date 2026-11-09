//! Tauri commands. These are thin: they validate, call into `glint-core`, and emit
//! events. Launcher logic belongs in the core crate.

pub mod auth;
pub mod instances;
pub mod launch;
pub mod meta;
pub mod mods;
pub mod system;

pub type CmdResult<T> = Result<T, glint_core::Error>;

/// Refuse an operation that would fight with a running or installing game.
pub fn ensure_idle(state: &crate::state::AppState, id: &str, action: &str) -> CmdResult<()> {
    if state.activity.lock().unwrap().contains_key(id) {
        return Err(glint_core::Error::Launch(format!(
            "Stop the game before you {action} this instance."
        )));
    }
    Ok(())
}
