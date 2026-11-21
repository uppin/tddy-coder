//! A session's **main** jail is watched for death (`#tab-hardening` 3/4).
//!
//! Once [`SandboxSessionManager::insert`] registers a main jail, [`watch_jail_exit`] waits on its
//! runner **without reaping it** (`waitid(WNOWAIT)` on a blocking task, no lock held across the
//! wait), so the exit status is never lost to a concurrent [`SandboxSessionState::is_running`]. A
//! runner that exits on its own then:
//!
//! - has its [`ChildExit`] recorded on its state ([`SandboxSessionState::exit`]), which ends its
//!   terminal — [`SandboxSessionState::subscribe_exit`] is what the session's `pty_done` follows;
//! - leaves the registry, when it is still the jail registered for its session;
//! - is announced on [`SandboxSessionManager::subscribe_ended`] as a [`JailEnded`].
//!
//! A jail stopped by request (delete, resume, relaunch, shutdown) is none of these: its stop was
//! asked for. Tab jails keep their own death watch ([`crate::tab_jails::TabJails`]).
//!
//! The runner is held as a [`JailProcess`] — an owned child today — so a jail a supervisor spawns
//! under another uid can plug in later (jail-uid, `docs/dev/todo/`).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-jail-death-watch.md

use std::sync::Arc;

use tddy_task::ChildExit;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

use crate::sandbox_session::{SandboxSessionManager, SandboxSessionState};

/// A main jail whose runner exited on its own: the session it served, and how the runner ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JailEnded {
    pub session_id: String,
    pub exit: ChildExit,
}

/// The process a jail runs as.
///
/// An owned child is the only kind today: the daemon spawned the runner and is its parent, so it can
/// wait on it without reaping and kill it. A jail spawned by the root supervisor under the session's
/// uid will be another variant (jail-uid).
#[derive(Debug)]
pub enum JailProcess {
    Owned(std::process::Child),
}

impl From<tddy_sandbox::SandboxHandle> for JailProcess {
    fn from(handle: tddy_sandbox::SandboxHandle) -> Self {
        JailProcess::Owned(handle.into_child())
    }
}

impl JailProcess {
    /// The runner's pid. Only meaningful while the runner is unreaped: it names the child this
    /// process owns, never a pid the OS reissued.
    pub fn pid(&self) -> u32 {
        // TODO(jail-death-watch): implement
        todo!("JailProcess::pid")
    }

    /// How the runner ended, if it has — read without reaping it (`waitid(WNOHANG | WNOWAIT)`), so
    /// the status stays for whoever waits on it next. `None` while it runs.
    pub fn try_exit(&mut self) -> Option<ChildExit> {
        // TODO(jail-death-watch): implement
        todo!("JailProcess::try_exit")
    }

    /// Kill the runner and reap it; how it ended, `None` when it could not be reaped.
    pub fn kill_and_reap(&mut self) -> Option<ChildExit> {
        // TODO(jail-death-watch): implement
        todo!("JailProcess::kill_and_reap")
    }
}

/// Block until the child `pid` of this process exits, and return how it ended **without reaping
/// it** (`waitid(P_PID, pid, WEXITED | WNOWAIT)`): the zombie stays for its owner to reap. Run it on
/// a blocking task, with no lock held.
///
/// An error when `pid` is not an unreaped child of this process — it was reaped meanwhile, so its
/// status is wherever its reaper recorded it.
pub fn wait_without_reaping(pid: u32) -> std::io::Result<ChildExit> {
    // TODO(jail-death-watch): implement
    let _ = pid;
    todo!("wait_without_reaping")
}

/// Wait until `state`'s runner exits, without reaping it and holding no lock across the wait; record
/// its exit on `state` (which ends its terminal). `Some(exit)` when it exited on its own, `None` when
/// it was stopped by request — before or during the wait.
pub async fn watch_jail_exit(state: Arc<SandboxSessionState>) -> Option<ChildExit> {
    // TODO(jail-death-watch): implement
    let _ = state;
    todo!("watch_jail_exit")
}

/// The tasks that bridge a jail's stdio to the daemon — the stdio RPC endpoint, the host relay and
/// the terminal-event forwarder — kept with the jail so its stop stops them
/// ([`SandboxSessionState::with_bridge`]).
#[derive(Debug)]
pub struct JailBridge {
    endpoint: JoinHandle<()>,
    relay: JoinHandle<()>,
    forwarder: JoinHandle<()>,
}

impl JailBridge {
    /// The bridge of a jail whose stdio endpoint, relay and terminal forwarder run as these tasks.
    pub fn new(endpoint: JoinHandle<()>, relay: JoinHandle<()>, forwarder: JoinHandle<()>) -> Self {
        JailBridge {
            endpoint,
            relay,
            forwarder,
        }
    }

    /// Abort every task of the bridge.
    pub fn abort(&self) {
        // TODO(jail-death-watch): implement
        let _ = (&self.endpoint, &self.relay, &self.forwarder);
        todo!("JailBridge::abort")
    }
}

impl SandboxSessionState {
    /// This state, holding `bridge`: [`Self::stop`] aborts its tasks.
    #[must_use]
    pub fn with_bridge(self, bridge: JailBridge) -> Self {
        // TODO(jail-death-watch): implement
        let _ = bridge;
        todo!("SandboxSessionState::with_bridge")
    }

    /// How the runner ended — on its own or stopped — once that is known; `None` while it runs.
    pub fn exit(&self) -> Option<ChildExit> {
        // TODO(jail-death-watch): implement
        todo!("SandboxSessionState::exit")
    }

    /// A watch of [`Self::exit`]: `Some` once the runner has ended — what the jail's terminal follows
    /// to end (`pty_done`).
    pub fn subscribe_exit(&self) -> watch::Receiver<Option<ChildExit>> {
        // TODO(jail-death-watch): implement
        todo!("SandboxSessionState::subscribe_exit")
    }
}

impl SandboxSessionManager {
    /// Every main jail whose runner exits on its own from now on, once each. A jail stopped by
    /// request is never announced.
    pub fn subscribe_ended(&self) -> broadcast::Receiver<JailEnded> {
        // TODO(jail-death-watch): implement
        todo!("SandboxSessionManager::subscribe_ended")
    }
}

#[cfg(all(test, unix))]
#[path = "jail_exit_watch_tests.rs"]
mod tests;
