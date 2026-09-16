//! The task body that owns the index-daemon child process for its whole lifetime.
//!
//! Modelled on [`tddy_lsp::server_body::LspServerBody`], and deliberately not on the sandbox
//! runner's relay: the runner's `JoinHandle` is discarded, so nothing watches that child for death
//! (`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`). The three
//! things taken from the LSP body are the three that entry is missing:
//!
//! 1. the child's pid is registered with the task, so the registry's SIGTERM→SIGKILL escalation
//!    net can reach it even if this body never gets to run its own cleanup;
//! 2. the exit is **observed** — `child.wait()` is one arm of a `select!`, so a process that dies
//!    on its own is noticed at once rather than discovered by the next caller;
//! 3. cancellation asks the child to stop and gives it a bounded grace period before killing it,
//!    because the index daemon has language servers of its own to take with it.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tddy_task::{TaskBody, TaskContext, TaskStatus};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::oneshot;

/// Log target for this body and for everything the child writes to its stderr, so a log policy can
/// separate the child's narration from the daemon's own.
const LOG_TARGET: &str = "tddy_daemon::index_daemon_body";

/// How long a cancelled index daemon is given to exit on the signal its own shutdown path listens
/// for, before this body kills it. The same 500 ms `LspServerBody` allows a wedged server.
const GRACEFUL_SHUTDOWN: Duration = Duration::from_millis(500);

/// How long the stderr drain is given to finish reading a dead child's last words. Without it the
/// reason a child died races the `wait()` that noticed it, and the reason usually loses.
const LAST_WORDS: Duration = Duration::from_millis(500);

/// A [`TaskBody`] hosting one `tddy-index-daemon` process.
pub struct IndexDaemonBody {
    /// The program to run.
    pub program: PathBuf,
    /// The AF_UNIX socket it is told to bind, which is also how the daemon reaches it.
    pub socket_path: PathBuf,
    /// Reports whether the process started at all, so "could not be executed" reaches the caller
    /// as itself rather than as a readiness failure. Sent before anything else happens.
    pub started_tx: oneshot::Sender<Result<(), String>>,
}

/// The last non-empty line the child wrote to stderr, shared with the drain task.
type LastWord = Arc<Mutex<Option<String>>>;

#[async_trait]
impl TaskBody for IndexDaemonBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let IndexDaemonBody {
            program,
            socket_path,
            started_tx,
        } = *self;

        let mut command = Command::new(&program);
        command
            .arg("--grpc-uds")
            .arg(&socket_path)
            // The index daemon serves on the socket above, so its own stdin and stdout carry
            // nothing — and leaving stdout inherited would let a library that forgets itself print
            // into whatever this daemon's stdout is.
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => {
                let _ = started_tx.send(Ok(()));
                child
            }
            Err(err) => {
                let message = format!(
                    "the index daemon '{}' could not be started: {err}",
                    program.display()
                );
                let _ = started_tx.send(Err(message.clone()));
                return TaskStatus::Failed { message };
            }
        };

        let pid = child.id();
        if let Some(pid) = pid {
            ctx.register_child_pid(pid);
        }

        let last_word: LastWord = Arc::new(Mutex::new(None));
        let mut draining = drain_stderr_into_the_log(&mut child, Arc::clone(&last_word));

        let cancel = ctx.cancel_token();
        tokio::select! {
            _ = cancel.cancelled() => {}
            waited = child.wait() => {
                // Give the drain a moment to reach end of file, so the exit is reported with
                // whatever the child said on its way out.
                let _ = tokio::time::timeout(LAST_WORDS, &mut draining).await;
                draining.abort();
                if ctx.is_cancelled() {
                    return TaskStatus::Cancelled;
                }
                return exited_on_its_own(waited, &last_word);
            }
        }

        // Cancelled: ask first — the index daemon's own shutdown path listens for SIGTERM and uses
        // it to unbind its socket and take its language servers with it — then make sure it is
        // gone. A child left behind here is the defect this body exists not to reproduce.
        if let Some(pid) = pid {
            ask_the_child_to_stop(pid);
        }
        if tokio::time::timeout(GRACEFUL_SHUTDOWN, child.wait())
            .await
            .is_err()
        {
            log::warn!(
                target: LOG_TARGET,
                "the index daemon did not stop within {}ms of being asked; killing it",
                GRACEFUL_SHUTDOWN.as_millis()
            );
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        draining.abort();
        TaskStatus::Cancelled
    }
}

/// Read the child's stderr line by line into the log, keeping the last thing it said.
///
/// Piping it and *not* reading it is the deadlock `LspServerBody` documents: the pipe buffer fills
/// at ~64 KiB and the child blocks mid-write. It goes to the log rather than to this process's
/// stderr so that every line carries the target that says which process wrote it.
fn drain_stderr_into_the_log(
    child: &mut tokio::process::Child,
    last_word: LastWord,
) -> tokio::task::JoinHandle<()> {
    let stderr = child.stderr.take();
    tokio::spawn(async move {
        let Some(stderr) = stderr else {
            return;
        };
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            log::info!(target: LOG_TARGET, "{line}");
            if let Ok(mut slot) = last_word.lock() {
                *slot = Some(line);
            }
        }
    })
}

/// The terminal status for an index daemon that ended without being asked to.
///
/// [`TaskStatus::Failed`] whatever the exit code, because this process is supposed to serve until
/// it is stopped: a clean `exit 0` from a daemon nobody stopped is still a daemon that is gone.
/// The message carries the decoded reason so the caller that was waiting for it can say what
/// happened rather than that something timed out.
fn exited_on_its_own(
    waited: std::io::Result<std::process::ExitStatus>,
    last_word: &Mutex<Option<String>>,
) -> TaskStatus {
    let how = match waited {
        Ok(status) => match status.code() {
            Some(code) => format!("exited with status {code}"),
            None => "was terminated by a signal".to_string(),
        },
        Err(err) => format!("could not be waited for: {err}"),
    };
    let said = last_word.lock().ok().and_then(|slot| slot.clone());
    let message = match said {
        Some(line) => format!("the index daemon {how}: {line}"),
        None => format!("the index daemon {how}"),
    };
    TaskStatus::Failed { message }
}

/// Ask the child to stop, using the signal its own shutdown path selects on.
///
/// The process itself rather than its process group: the precedent is `LspServerBody`, which
/// signals the one child it spawned, and the index daemon's children are its own to shut down —
/// `serve.rs` cancels their tasks, which is what kills them.
#[cfg(unix)]
fn ask_the_child_to_stop(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
}

/// Refused rather than faked: without a signal there is no graceful stop, only the kill below.
#[cfg(not(unix))]
fn ask_the_child_to_stop(_pid: u32) {}
