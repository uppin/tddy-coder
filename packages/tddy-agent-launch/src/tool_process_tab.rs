//! A tool session's `tddy-coder` as a process tab of its session (`#session-liveness` 3/4): the
//! holders that stop and signal it, one per spawn backend, and the flag that says the child ended.
//!
//! - **supervisor** — the supervisor owns the child, so stopping is its `StopSession` and a signal
//!   is its `SignalSession`; it signals the pid it spawned and no other.
//! - **forked spawn worker and direct spawn** — the child is the daemon's own descendant, so the
//!   daemon signals its pid. The pid is valid until the backend reports the child's end, which
//!   [`observe`] records, so nothing is sent to a pid after the child is known to be gone.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tddy_cli_sessions::cli_session_manager::ProcessTabHolder;
use tddy_spawn::child_exit::SpawnedTool;
use tddy_task::{exit_channel, ExitReceiver};

/// How long a child that was asked to stop gets before it is killed — the grace the supervisor's
/// own stop gives a session, and delete gave a recorded pid.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// Whether the child's holder has reported its end.
pub(crate) type EndedFlag = Arc<AtomicBool>;

/// Who holds a spawned `tddy-coder`.
#[derive(Clone)]
pub(crate) enum ToolBackend {
    /// `tddy-supervisor`, reached at this socket.
    Supervisor { socket_path: PathBuf },
    /// The forked spawn worker, or a direct spawn: a descendant of the daemon.
    Owned,
}

/// A spawned `tddy-coder` and the backend that holds it.
pub struct SpawnedToolChild {
    pub(crate) tool: SpawnedTool,
    pub(crate) backend: ToolBackend,
}

/// Watch `exit` on the side: the returned receiver resolves exactly as `exit` does, and the flag
/// is set before it resolves.
pub(crate) fn observe(exit: ExitReceiver) -> (ExitReceiver, EndedFlag) {
    let ended: EndedFlag = Arc::default();
    let (holder, observed) = exit_channel();
    let flag = Arc::clone(&ended);
    tokio::spawn(async move {
        let end = exit.exited().await;
        flag.store(true, Ordering::SeqCst);
        // A holder that went away reported nothing: the dropped sender says so to the tab.
        if let Ok(end) = end {
            holder.send(end);
        }
    });
    (observed, ended)
}

impl ToolBackend {
    /// The holder of a child `pid` this backend started; `ended` is [`observe`]'s flag for it.
    pub(crate) fn holder(&self, pid: u32, ended: EndedFlag) -> Arc<dyn ProcessTabHolder> {
        match self {
            Self::Supervisor { socket_path } => Arc::new(SupervisedChild {
                socket_path: socket_path.clone(),
                pid,
            }),
            Self::Owned => Arc::new(OwnedChild { pid, ended }),
        }
    }
}

/// A child the supervisor holds.
struct SupervisedChild {
    socket_path: PathBuf,
    pid: u32,
}

#[async_trait::async_trait]
impl ProcessTabHolder for SupervisedChild {
    async fn stop(&self) -> anyhow::Result<()> {
        tddy_spawn::supervisor_client::connect_supervisor(&self.socket_path)
            .await?
            .stop_session(self.pid)
            .await?;
        Ok(())
    }

    async fn signal(&self, signal: i32) -> anyhow::Result<()> {
        tddy_spawn::supervisor_client::connect_supervisor(&self.socket_path)
            .await?
            .signal_session(self.pid, signal)
            .await?;
        Ok(())
    }
}

/// A child that is a descendant of the daemon.
struct OwnedChild {
    pid: u32,
    ended: EndedFlag,
}

impl OwnedChild {
    fn has_ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }

    fn deliver(&self, signal: i32) -> anyhow::Result<()> {
        deliver(self.pid, signal)
    }
}

fn deliver(pid: u32, signal: i32) -> anyhow::Result<()> {
    // SAFETY: `kill` only inspects the pid table. The pid is the child's own, and the caller has
    // checked that its holder has not reported the child's end, so it has not been recycled.
    if unsafe { libc::kill(pid as libc::pid_t, signal) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    anyhow::bail!("kill({pid}, {signal}): {error}")
}

#[async_trait::async_trait]
impl ProcessTabHolder for OwnedChild {
    /// SIGTERM now, SIGKILL after [`STOP_GRACE`] unless the child has ended by then.
    async fn stop(&self) -> anyhow::Result<()> {
        if self.has_ended() {
            return Ok(());
        }
        self.deliver(libc::SIGTERM)?;
        let (pid, ended) = (self.pid, Arc::clone(&self.ended));
        tokio::spawn(async move {
            tokio::time::sleep(STOP_GRACE).await;
            if !ended.load(Ordering::SeqCst) {
                log::warn!("tool child pid={pid} outlived its grace period; killing it");
                if let Err(error) = deliver(pid, libc::SIGKILL) {
                    log::warn!("tool child pid={pid} could not be killed: {error}");
                }
            }
        });
        Ok(())
    }

    async fn signal(&self, signal: i32) -> anyhow::Result<()> {
        if self.has_ended() {
            anyhow::bail!("the child has ended");
        }
        self.deliver(signal)
    }
}
