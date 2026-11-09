use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Mutex,
};

use glint_core::{
    auth::{SignedIn, store::KeyringStore},
    launch::{GameHandle, LogLine},
    paths::Paths,
};
use tokio::task::AbortHandle;

/// How many log lines are kept per instance for the log view to load when it opens.
pub const LOG_CAPACITY: usize = 5000;

/// What an instance that isn't idle is doing.
pub enum Activity {
    /// Installing or signing in. Aborting the task cancels it.
    Preparing(AbortHandle),
    Running(GameHandle),
}

pub struct AppState {
    pub paths: Paths,
    pub client: reqwest::Client,
    pub secrets: KeyringStore,
    pub total_memory_mb: u32,
    /// Minecraft sessions by account UUID, kept in memory only and reused until they
    /// are close to expiring.
    pub sessions: Mutex<HashMap<String, SignedIn>>,
    pub activity: Mutex<HashMap<String, Activity>>,
    pub logs: Mutex<HashMap<String, VecDeque<LogLine>>>,
    pub pending_login: Mutex<Option<AbortHandle>>,
    /// Serialises changes to mod folders and their indexes, so two installs can't
    /// overwrite each other's bookkeeping.
    pub mods_lock: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> glint_core::Result<Self> {
        Ok(Self {
            paths: Paths::new(data_dir),
            client: glint_core::http_client()?,
            secrets: KeyringStore,
            total_memory_mb: glint_core::system::total_memory_mb(),
            sessions: Mutex::default(),
            activity: Mutex::default(),
            logs: Mutex::default(),
            pending_login: Mutex::default(),
            mods_lock: tokio::sync::Mutex::default(),
        })
    }

    pub fn push_log(&self, instance_id: &str, lines: &[LogLine]) {
        let mut logs = self.logs.lock().unwrap();
        let buffer = logs.entry(instance_id.to_string()).or_default();
        buffer.extend(lines.iter().cloned());
        let overflow = buffer.len().saturating_sub(LOG_CAPACITY);
        buffer.drain(..overflow);
    }
}
