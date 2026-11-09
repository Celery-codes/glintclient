//! Tauri commands. These are thin: they validate, call into `glint-core`, and emit
//! events. Launcher logic belongs in the core crate.

pub mod auth;
pub mod instances;
pub mod launch;
pub mod meta;
pub mod system;

pub type CmdResult<T> = Result<T, glint_core::Error>;
