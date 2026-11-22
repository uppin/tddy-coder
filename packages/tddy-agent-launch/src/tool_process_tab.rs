//! A tool session's `tddy-coder` as a process tab of its session (`#session-liveness` 3/4): the
//! holders that stop and signal it, one per spawn backend, and the flag that says the child ended.
//!
//! - **supervisor** — the supervisor owns the child, so stopping is its `StopSession` and a signal
//!   is its `SignalSession`; it signals the pid it spawned and no other.
//! - **forked spawn worker and direct spawn** — the child is the daemon's own descendant, so the
//!   daemon signals its pid. The pid is valid until the backend reports the child's end, which
//!   [`observe`] records, so nothing is sent to a pid after the child is known to be gone. Each
//!   signal goes through the holder's `deliver` seam: the spawn's
//!   [`tddy_spawn::child_signal::ToolSignal`], which sends it only while the pid is still the
//!   child's and answers [`Signalled::ChildEnded`] once it is not (`#tab-hardening` 2/4).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tddy_cli_sessions::cli_session_manager::ProcessTabHolder;
use tddy_spawn::child_exit::SpawnedTool;
use tddy_spawn::child_signal::{Signalled, ToolSignal};
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
    /// The holder of a child `pid` this backend started; `ended` is [`observe`]'s flag for it, and
    /// `signal` the spawn's way of signalling a child the daemon or its worker is the parent of
    /// (`None` for a supervised child, which the supervisor signals).
    pub(crate) fn holder(
        &self,
        pid: u32,
        ended: EndedFlag,
        signal: Option<ToolSignal>,
    ) -> Arc<dyn ProcessTabHolder> {
        match self {
            Self::Supervisor { socket_path } => Arc::new(SupervisedChild {
                socket_path: socket_path.clone(),
                pid,
            }),
            Self::Owned => Arc::new(OwnedChild {
                pid,
                ended,
                deliver: Arc::new(move |number| match &signal {
                    Some(signal) => signal.send(number),
                    None => anyhow::bail!("the spawn of child {pid} gave no way to signal it"),
                }),
            }),
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

/// How an [`OwnedChild`]'s signal is sent, and whether it reached the child — the seam a test
/// replaces to stand in for the backend.
pub(crate) type Deliver = Arc<dyn Fn(i32) -> anyhow::Result<Signalled> + Send + Sync>;

/// A child that is a descendant of the daemon.
struct OwnedChild {
    pid: u32,
    ended: EndedFlag,
    deliver: Deliver,
}

impl OwnedChild {
    fn has_ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }

    /// Send `signal` through the seam, and say whether it reached the child.
    fn deliver(&self, signal: i32) -> anyhow::Result<Signalled> {
        (self.deliver)(signal)
    }
}

#[async_trait::async_trait]
impl ProcessTabHolder for OwnedChild {
    /// SIGTERM now, SIGKILL after [`STOP_GRACE`] unless the child has ended by then.
    async fn stop(&self) -> anyhow::Result<()> {
        if self.has_ended() {
            return Ok(());
        }
        if self.deliver(libc::SIGTERM)? == Signalled::ChildEnded {
            // The backend had already reaped it: nothing left to wait out, nothing to kill.
            return Ok(());
        }
        let (pid, ended, deliver) = (self.pid, Arc::clone(&self.ended), Arc::clone(&self.deliver));
        tokio::spawn(async move {
            tokio::time::sleep(STOP_GRACE).await;
            if !ended.load(Ordering::SeqCst) {
                log::warn!("tool child pid={pid} outlived its grace period; killing it");
                match deliver(libc::SIGKILL) {
                    Ok(_) => {}
                    Err(error) => log::warn!("tool child pid={pid} could not be killed: {error}"),
                }
            }
        });
        Ok(())
    }

    async fn signal(&self, signal: i32) -> anyhow::Result<()> {
        if self.has_ended() {
            anyhow::bail!("the child has ended");
        }
        match self.deliver(signal)? {
            Signalled::Delivered => Ok(()),
            Signalled::ChildEnded => anyhow::bail!("the child has ended"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A pid no test signals: the seam stands in for the backend, so it is never `kill`ed.
    const STAND_IN_PID: u32 = 4_000_000;

    /// An owned child whose backend answers every signal with `answer`, recording each it is sent.
    fn an_owned_child_whose_backend_answers(
        answer: Signalled,
    ) -> (OwnedChild, Arc<Mutex<Vec<i32>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&sent);
        let child = OwnedChild {
            pid: STAND_IN_PID,
            ended: Arc::default(),
            deliver: Arc::new(move |signal| {
                recorded.lock().unwrap().push(signal);
                Ok(answer)
            }),
        };
        (child, sent)
    }

    #[tokio::test]
    async fn a_signal_to_a_running_child_goes_through_its_backend() {
        // Given
        let (child, sent) = an_owned_child_whose_backend_answers(Signalled::Delivered);

        // When
        let signalled = child.signal(libc::SIGUSR1).await;

        // Then
        assert_eq!(
            (signalled.is_ok(), sent.lock().unwrap().clone()),
            (true, vec![libc::SIGUSR1])
        );
    }

    #[tokio::test]
    async fn a_signal_the_backend_found_the_child_ended_for_is_refused_as_ended() {
        // Given
        let (child, _sent) = an_owned_child_whose_backend_answers(Signalled::ChildEnded);

        // When
        let refusal = child
            .signal(libc::SIGUSR1)
            .await
            .expect_err("a signal that reached no child is refused");

        // Then
        assert!(
            refusal.to_string().contains("ended"),
            "the refusal says the child ended: {refusal}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stopping_a_child_the_backend_found_ended_is_done_and_kills_nothing() {
        // Given
        let (child, sent) = an_owned_child_whose_backend_answers(Signalled::ChildEnded);

        // When
        let stopped = child.stop().await;
        tokio::time::sleep(STOP_GRACE + Duration::from_secs(1)).await;

        // Then
        assert_eq!(
            (stopped.is_ok(), sent.lock().unwrap().clone()),
            (true, vec![libc::SIGTERM])
        );
    }
}
