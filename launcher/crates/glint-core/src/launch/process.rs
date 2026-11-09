use std::{process::Stdio, sync::Arc, time::Duration};

use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
    sync::{mpsc, oneshot},
};

use super::args::LaunchCommand;
use crate::{Error, Result};

/// Lines are delivered in batches so a chatty game can't flood the UI with events.
const BATCH_INTERVAL: Duration = Duration::from_millis(40);
const MAX_BATCH: usize = 512;

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub text: String,
    /// True for stderr.
    pub err: bool,
}

#[derive(Debug, Clone)]
pub enum GameEvent {
    Log(Vec<LogLine>),
    /// The process ended. `code` is `None` if it was killed by a signal.
    Exit {
        code: Option<i32>,
        killed: bool,
    },
}

/// A running game. Dropping the handle leaves the game running.
#[derive(Debug)]
pub struct GameHandle {
    pub pid: Option<u32>,
    kill: Option<oneshot::Sender<()>>,
}

impl GameHandle {
    /// Ask the game to stop. The `Exit` event still follows.
    pub fn kill(&mut self) {
        if let Some(kill) = self.kill.take() {
            let _ = kill.send(());
        }
    }
}

/// Start the game and stream its output to `on_event` until it exits.
pub fn spawn(
    command: LaunchCommand,
    on_event: impl Fn(GameEvent) + Send + Sync + 'static,
) -> Result<GameHandle> {
    let mut cmd = Command::new(&command.program);
    cmd.args(&command.args)
        .current_dir(&command.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: never flash a console, even if a custom Java path points at
        // java.exe rather than javaw.exe.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| {
        Error::Launch(format!(
            "couldn't start Java at {}: {e}",
            command.program.display()
        ))
    })?;
    let pid = child.id();
    let on_event = Arc::new(on_event);

    let (line_tx, mut line_rx) = mpsc::unbounded_channel::<LogLine>();
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(read_lines(stdout, false, line_tx.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(read_lines(stderr, true, line_tx.clone()));
    }
    drop(line_tx);

    // Forward log lines in batches. Ends when both pipes close, i.e. the game is gone.
    let log_events = on_event.clone();
    let log_task = tokio::spawn(async move {
        let mut batch = Vec::new();
        while line_rx.recv_many(&mut batch, MAX_BATCH).await > 0 {
            log_events(GameEvent::Log(std::mem::take(&mut batch)));
            tokio::time::sleep(BATCH_INTERVAL).await;
        }
    });

    let (kill_tx, kill_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let mut killed = false;
        let status = tokio::select! {
            status = child.wait() => status,
            // Fires on an explicit kill only; a dropped handle must not stop the game.
            Ok(()) = kill_rx => {
                killed = true;
                let _ = child.start_kill();
                child.wait().await
            }
        };
        // Let the last lines through before announcing the exit. Bounded, because a
        // child process the game left behind could hold the pipes open indefinitely.
        let _ = tokio::time::timeout(Duration::from_secs(2), log_task).await;
        on_event(GameEvent::Exit {
            code: status.ok().and_then(|s| s.code()),
            killed,
        });
    });

    Ok(GameHandle {
        pid,
        kill: Some(kill_tx),
    })
}

async fn read_lines(pipe: impl AsyncRead + Unpin, err: bool, tx: mpsc::UnboundedSender<LogLine>) {
    let mut reader = BufReader::new(pipe);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                // The game may write in the system code page, so decode leniently
                // instead of failing on a non-UTF-8 byte.
                let text = String::from_utf8_lossy(&buf);
                let text = text.trim_end_matches(['\r', '\n']).to_string();
                if tx.send(LogLine { text, err }).is_err() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn shell(script: &str) -> LaunchCommand {
        let (program, flag) = if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };
        LaunchCommand {
            program: program.into(),
            args: vec![flag.into(), script.into()],
            cwd: std::env::temp_dir(),
        }
    }

    async fn collect(
        command: LaunchCommand,
        kill_after: Option<Duration>,
    ) -> (Vec<LogLine>, Option<i32>, bool) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let (done_tx, done_rx) = oneshot::channel();
        let done_tx = Mutex::new(Some(done_tx));
        let sink = lines.clone();
        let mut handle = spawn(command, move |event| match event {
            GameEvent::Log(batch) => sink.lock().unwrap().extend(batch),
            GameEvent::Exit { code, killed } => {
                if let Some(tx) = done_tx.lock().unwrap().take() {
                    let _ = tx.send((code, killed));
                }
            }
        })
        .unwrap();
        if let Some(delay) = kill_after {
            tokio::time::sleep(delay).await;
            handle.kill();
        }
        let (code, killed) = tokio::time::timeout(Duration::from_secs(20), done_rx)
            .await
            .expect("process should exit")
            .unwrap();
        let lines = lines.lock().unwrap().clone();
        (lines, code, killed)
    }

    #[tokio::test]
    async fn streams_both_pipes_then_reports_exit_code() {
        let script = if cfg!(windows) {
            "echo out-line& echo err-line 1>&2& exit 3"
        } else {
            "echo out-line; echo err-line 1>&2; exit 3"
        };
        let (lines, code, killed) = collect(shell(script), None).await;
        assert_eq!(code, Some(3));
        assert!(!killed);
        assert!(lines.iter().any(|l| l.text.trim() == "out-line" && !l.err));
        assert!(lines.iter().any(|l| l.text.trim() == "err-line" && l.err));
    }

    #[tokio::test]
    async fn kill_stops_a_running_process() {
        // Run the long-lived program directly: killing a shell would orphan its child.
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("ping", &["-n", "30", "127.0.0.1"])
        } else {
            ("sleep", &["30"])
        };
        let command = LaunchCommand {
            program: program.into(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            cwd: std::env::temp_dir(),
        };
        let started = std::time::Instant::now();
        let (_, _, killed) = collect(command, Some(Duration::from_millis(300))).await;
        assert!(killed);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn missing_program_is_a_launch_error() {
        let command = LaunchCommand {
            program: "definitely-not-a-real-java".into(),
            args: vec![],
            cwd: std::env::temp_dir(),
        };
        assert!(matches!(spawn(command, |_| {}), Err(Error::Launch(_))));
    }
}
