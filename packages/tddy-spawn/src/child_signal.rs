//! Signalling a tool child the daemon — or its forked spawn worker — is the parent of, never a pid
//! the kernel has handed to another process (`#tab-hardening` 2/4).
//!
//! A child's pid stays its own until its parent reaps it. So the thread that reaps a child first
//! waits for it **without** reaping it (`waitid` with `WNOWAIT`), then — under the [`ReapGate`]'s
//! lock — marks it ended and reaps it. Every signal is sent under the same lock, after checking the
//! mark: a signal is therefore sent either while the pid still names the child (alive, or a zombie
//! nobody has reaped), or not at all.
//!
//! - **direct spawn** — the daemon is the parent: [`ToolSignal::Direct`] signals under the gate its
//!   own reaper thread holds.
//! - **forked spawn worker** — the worker is the parent, so only it can know whether it has reaped
//!   the child: [`ToolSignal::Worker`] asks it with [`crate::spawn_worker::WorkerRequest::Signal`],
//!   which the worker answers under the gate of the child it holds.
//! - **supervisor** — the supervisor signals the pid it spawned and no other; it has no
//!   [`ToolSignal`].

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::spawn_worker::SpawnClient;

/// Whether a signal reached the child.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signalled {
    /// Sent while the pid was still the child's.
    #[serde(rename = "delivered")]
    Delivered,
    /// Not sent: the child had ended, so its pid may already name another process.
    #[serde(rename = "child_ended")]
    ChildEnded,
}

/// The lock a child's reaper and every signal to that child share, and the mark the reaper sets
/// under it before the child's pid is freed. Clones share one gate.
#[derive(Debug, Clone, Default)]
pub struct ReapGate {
    ended: Arc<Mutex<bool>>,
}

impl ReapGate {
    /// A gate for a child that has not ended.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Send `signal` through `deliver` unless the child has been marked ended, holding the gate's
    /// lock across the check and the delivery. `deliver` is the seam the send goes through —
    /// `kill(2)` of the child's pid in production.
    pub fn signal_with(
        &self,
        signal: i32,
        deliver: impl FnOnce(i32) -> std::io::Result<()>,
    ) -> std::io::Result<Signalled> {
        let ended = self.lock();
        if *ended {
            return Ok(Signalled::ChildEnded);
        }
        deliver(signal)?;
        Ok(Signalled::Delivered)
    }

    /// Mark the child ended, then run `reap` — which frees its pid — still holding the gate's lock,
    /// and hand back what `reap` returned. Called once, by the child's reaper, after it has waited
    /// for the child without reaping it ([`wait_without_reaping`]).
    pub fn mark_ended_then_reap<T>(&self, reap: impl FnOnce() -> T) -> T {
        let mut ended = self.lock();
        *ended = true;
        reap()
    }

    /// The lock, whose mark is a plain flag: a thread that panicked holding it left it consistent.
    fn lock(&self) -> std::sync::MutexGuard<'_, bool> {
        self.ended.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Block until the child `pid` of this process has ended, leaving it unreaped — a zombie whose pid
/// no other process can be given — so the caller can mark it ended before reaping it
/// (`waitid(P_PID, pid, WEXITED | WNOWAIT)`).
#[cfg(unix)]
pub fn wait_without_reaping(pid: u32) -> std::io::Result<()> {
    loop {
        // SAFETY: `info` is plain data the kernel fills in; `waitid` reads nothing else of ours.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let rc = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if rc == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// `kill(2)` of `pid`, the delivery a [`ReapGate`] guards.
#[cfg(unix)]
fn kill_pid(pid: u32, signal: i32) -> std::io::Result<()> {
    // SAFETY: `kill` takes two integers and touches no memory of ours.
    if unsafe { libc::kill(pid as libc::pid_t, signal) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// The gates of the children a forked spawn worker holds, by pid, so a [`WorkerRequest::Signal`]
/// finds the lock the child's reaper takes. An entry stays after its child ended — the gate then
/// answers [`Signalled::ChildEnded`] — until a child that is given the same pid replaces it.
#[cfg(unix)]
pub(crate) mod worker_gates {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::ReapGate;

    static GATES: Mutex<Option<HashMap<u32, ReapGate>>> = Mutex::new(None);

    fn table() -> std::sync::MutexGuard<'static, Option<HashMap<u32, ReapGate>>> {
        GATES.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Remember the gate of the child `pid`.
    pub(crate) fn hold(pid: u32, gate: ReapGate) {
        table().get_or_insert_with(HashMap::new).insert(pid, gate);
    }

    /// The gate of the child `pid`, when this worker spawned it.
    pub(crate) fn of(pid: u32) -> Option<ReapGate> {
        table().as_ref().and_then(|gates| gates.get(&pid).cloned())
    }
}

/// How the daemon signals a tool child it, or its forked spawn worker, is the parent of.
#[derive(Clone)]
pub enum ToolSignal {
    /// A child the daemon spawned itself: signalled under the gate its reaper thread holds.
    Direct { pid: u32, gate: ReapGate },
    /// A child the forked spawn worker spawned: the worker signals it under its own gate.
    Worker { pid: u32, client: SpawnClient },
}

impl ToolSignal {
    /// The pid of the child this signals.
    #[must_use]
    pub fn pid(&self) -> u32 {
        match self {
            Self::Direct { pid, .. } | Self::Worker { pid, .. } => *pid,
        }
    }

    /// Send `signal` to the child while its pid is still its own; [`Signalled::ChildEnded`] once
    /// its parent has marked it ended. Blocking: the worker variant waits for the worker's answer.
    pub fn send(&self, signal: i32) -> anyhow::Result<Signalled> {
        match self {
            #[cfg(unix)]
            Self::Direct { pid, gate } => Ok(gate.signal_with(signal, |s| kill_pid(*pid, s))?),
            #[cfg(not(unix))]
            Self::Direct { .. } => anyhow::bail!("signalling a child is not supported here"),
            Self::Worker { pid, client } => client.signal_child(*pid, signal),
        }
    }

    /// What the forked worker does for [`crate::spawn_worker::WorkerRequest::Signal`]: signal `pid`
    /// under the gate of the child it holds.
    #[cfg(unix)]
    pub(crate) fn send_from_worker(pid: u32, signal: i32) -> anyhow::Result<Signalled> {
        let gate = worker_gates::of(pid)
            .ok_or_else(|| anyhow::anyhow!("pid {pid} is not a child of this spawn worker"))?;
        Ok(gate.signal_with(signal, |s| kill_pid(pid, s))?)
    }
}

impl std::fmt::Debug for ToolSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct { pid, .. } => f.debug_struct("Direct").field("pid", pid).finish(),
            Self::Worker { pid, .. } => f.debug_struct("Worker").field("pid", pid).finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::sync::Mutex as StdMutex;

    use super::*;

    /// A `deliver` seam that records each signal it is handed and reports it sent.
    fn a_recording_deliver(
        sent: &StdMutex<Vec<i32>>,
    ) -> impl FnOnce(i32) -> std::io::Result<()> + '_ {
        move |signal| {
            sent.lock().unwrap().push(signal);
            Ok(())
        }
    }

    #[test]
    fn a_signal_to_a_child_that_has_not_ended_is_delivered() {
        // Given
        let gate = ReapGate::new();
        let sent = StdMutex::new(Vec::new());

        // When
        let outcome = gate
            .signal_with(libc::SIGTERM, a_recording_deliver(&sent))
            .unwrap();

        // Then
        assert_eq!(
            (outcome, sent.into_inner().unwrap()),
            (Signalled::Delivered, vec![libc::SIGTERM])
        );
    }

    #[test]
    fn a_signal_after_the_reaper_marked_the_child_ended_is_not_delivered() {
        // Given
        let gate = ReapGate::new();
        gate.mark_ended_then_reap(|| ());
        let sent = StdMutex::new(Vec::new());

        // When
        let outcome = gate
            .signal_with(libc::SIGTERM, a_recording_deliver(&sent))
            .unwrap();

        // Then
        assert_eq!(
            (outcome, sent.into_inner().unwrap()),
            (Signalled::ChildEnded, Vec::<i32>::new())
        );
    }

    #[test]
    fn the_child_is_marked_ended_before_its_pid_is_reaped() {
        // Given a signal sent from another thread while the reaper is reaping
        let gate = ReapGate::new();
        let (outcome_tx, outcome_rx) = mpsc::channel();

        // When
        let answered_during_the_reap = gate.mark_ended_then_reap(|| {
            let signaller = gate.clone();
            std::thread::spawn(move || {
                let delivered = StdMutex::new(Vec::new());
                let outcome = signaller
                    .signal_with(libc::SIGTERM, a_recording_deliver(&delivered))
                    .unwrap();
                outcome_tx
                    .send((outcome, delivered.into_inner().unwrap()))
                    .unwrap();
            });
            // The gate's lock is held for the whole reap, so the signal cannot be answered yet.
            outcome_rx
                .recv_timeout(std::time::Duration::from_millis(200))
                .is_ok()
        });
        let (outcome, delivered) = outcome_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the signal is answered once the reap is done");

        // Then it waited for the reap and found the child ended, so nothing reached the freed pid
        assert_eq!(
            (answered_during_the_reap, outcome, delivered),
            (false, Signalled::ChildEnded, Vec::<i32>::new())
        );
    }

    #[test]
    fn a_failed_delivery_is_reported_as_the_os_error() {
        // Given
        let gate = ReapGate::new();

        // When
        let outcome = gate.signal_with(libc::SIGTERM, |_| {
            Err(std::io::Error::from_raw_os_error(libc::EPERM))
        });

        // Then
        assert_eq!(
            outcome.map_err(|e| e.raw_os_error()),
            Err(Some(libc::EPERM))
        );
    }

    #[cfg(unix)]
    #[test]
    fn waiting_without_reaping_leaves_the_ended_child_reapable() {
        // Given a child that exits with a code
        let mut child = std::process::Command::new("sh")
            .args(["-c", "exit 7"])
            .spawn()
            .unwrap();
        let pid = child.id();

        // When
        wait_without_reaping(pid).unwrap();

        // Then its status is still there to be reaped
        assert_eq!(child.wait().unwrap().code(), Some(7));
    }
}
