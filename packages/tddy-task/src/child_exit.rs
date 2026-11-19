//! How a child process ended, as reported by whoever holds it.
//!
//! The daemon is not always the parent of the processes its sessions run: a tool session's
//! `tddy-coder` is held by the forked spawn worker, by `tddy-supervisor`, or by a direct spawn's wait
//! thread. Each holder reports the child's end **once**, through an [`ExitSender`]; the daemon side
//! keeps the matching [`ExitReceiver`] — what a process tab of the session waits on
//! (`#session-liveness` 3/4).
//!
//! It lives here, beside [`crate::TaskRegistry`], rather than in `tddy-spawn`: the terminal registry
//! (`tddy-cli-sessions`) holds the receiver, and it must not depend on the spawn backends.

use tokio::sync::oneshot;

/// How a held child ended. Exactly one of the two is set for a child the holder reaped: its exit
/// code, or the signal that killed it. A holder that knows only that the child is gone (the
/// supervisor reports no signal number) sets neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildExit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

impl ChildExit {
    /// The child exited with `code`.
    pub fn with_code(code: i32) -> Self {
        Self {
            code: Some(code),
            signal: None,
        }
    }

    /// The child was killed by `signal`.
    pub fn by_signal(signal: i32) -> Self {
        Self {
            code: None,
            signal: Some(signal),
        }
    }
}

/// The holder dropped its [`ExitSender`] without reporting an exit — it went away (a dead spawn
/// worker, a lost supervisor connection), and with it the daemon's only knowledge of the child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HolderGone;

impl std::fmt::Display for HolderGone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the child's holder went away without reporting its exit")
    }
}

impl std::error::Error for HolderGone {}

/// The holder's half: reports the child's end once, consuming itself.
#[derive(Debug)]
pub struct ExitSender(oneshot::Sender<ChildExit>);

impl ExitSender {
    /// Report the child's end. A receiver that is already gone has nobody left to tell, which is
    /// not the holder's failure.
    pub fn send(self, exit: ChildExit) {
        let _ = self.0.send(exit);
    }
}

/// The daemon's half: resolves exactly once, with the child's end or [`HolderGone`].
#[derive(Debug)]
pub struct ExitReceiver(oneshot::Receiver<ChildExit>);

impl ExitReceiver {
    /// Wait for the child's end.
    pub async fn exited(self) -> Result<ChildExit, HolderGone> {
        self.0.await.map_err(|_| HolderGone)
    }
}

/// A fresh pair for one spawned child.
pub fn exit_channel() -> (ExitSender, ExitReceiver) {
    let (tx, rx) = oneshot::channel();
    (ExitSender(tx), ExitReceiver(rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_receiver_resolves_with_the_exit_its_holder_reported() {
        // Given
        let (holder, daemon) = exit_channel();

        // When
        holder.send(ChildExit::with_code(3));

        // Then
        assert_eq!(daemon.exited().await, Ok(ChildExit::with_code(3)));
    }

    #[tokio::test]
    async fn the_receiver_resolves_as_holder_gone_when_the_holder_drops_without_reporting() {
        // Given
        let (holder, daemon) = exit_channel();

        // When
        drop(holder);

        // Then
        assert_eq!(daemon.exited().await, Err(HolderGone));
    }
}
