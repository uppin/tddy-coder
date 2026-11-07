//! The spawn and cancel core every headless turn shares (`#agent-tools 15/19`): a child in **its
//! own process group**, with no inherited `TDDY_*` variable, whose group is killed — and only its
//! group — when the turn is cancelled or runs out of time.
//!
//! Replaces the global `CHILD_PID` slot, which one turn overwrote with the next and which killed a
//! single process, never its children. The groups of the turns now running are registered here so
//! a Ctrl+C in the TUI ([`kill_running_turns`]) reaches every one of them.

use std::collections::BTreeSet;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::error::BackendError;

/// The prefix of the variables a headless agent never sees: its session is found through the
/// session pointer, never through the environment.
pub(crate) const TDDY_ENV_PREFIX: &str = "TDDY_";

/// How long a finished turn waits for its stderr to close before it keeps what it has read: a
/// grandchild that inherited the pipe can hold it open. One grace for workflow and conversation turns.
pub(crate) const STDERR_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// The process groups of the turns running now.
static RUNNING_GROUPS: Mutex<BTreeSet<u32>> = Mutex::new(BTreeSet::new());

fn running_groups() -> std::sync::MutexGuard<'static, BTreeSet<u32>> {
    RUNNING_GROUPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Removes every `TDDY_*` variable this process carries from what `command` will inherit.
pub(crate) fn strip_tddy_env(command: &mut std::process::Command) {
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with(TDDY_ENV_PREFIX) {
            command.env_remove(&name);
        }
    }
}

/// How one turn's child is started.
pub(crate) struct TurnCommand<'a> {
    pub(crate) program: &'a Path,
    pub(crate) argv: &'a [String],
    /// Set on top of the inherited environment, in order, after `PATH`.
    pub(crate) env: &'a [(String, String)],
    pub(crate) cwd: &'a Path,
    pub(crate) inherit_stdin: bool,
}

/// The command one turn's child is started with: `turn.program` as the leader of a new process
/// group, `TDDY_*` variables stripped from what it inherits, stdout and stderr piped.
pub(crate) fn spawn_turn_command(turn: &TurnCommand<'_>) -> std::process::Command {
    use std::os::unix::process::CommandExt as _;
    let mut command = std::process::Command::new(turn.program);
    command.args(turn.argv).current_dir(turn.cwd);
    // TODO(env-allow-list): implement — `crate::agent_env::apply_agent_child_env`, not a deny-list.
    strip_tddy_env(&mut command);
    command.env("PATH", super::path_with_exe_dir());
    command.envs(turn.env.iter().map(|(name, value)| (name, value)));
    command
        .stdin(if turn.inherit_stdin {
            std::process::Stdio::inherit()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .process_group(0);
    command
}

/// Spawns [`spawn_turn_command`].
pub(crate) fn spawn_turn(turn: &TurnCommand<'_>) -> Result<std::process::Child, BackendError> {
    spawn_turn_command(turn).spawn().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            BackendError::BinaryNotFound(turn.program.to_string_lossy().to_string())
        } else {
            BackendError::InvocationFailed(format!(
                "could not start {}: {e}",
                turn.program.display()
            ))
        }
    })
}

/// A running turn's process group. Dropping it — the turn's future dropped, or the turn done —
/// kills the group unless the leader has been reaped, and unregisters it.
pub(crate) struct TurnGroup {
    pgid: u32,
    reaped: Arc<AtomicBool>,
}

impl TurnGroup {
    /// Registers the group `pgid` leads; `reaped` is set by whoever waits for the leader.
    pub(crate) fn new(pgid: u32) -> (Self, Arc<AtomicBool>) {
        running_groups().insert(pgid);
        let reaped = Arc::new(AtomicBool::new(false));
        (
            Self {
                pgid,
                reaped: Arc::clone(&reaped),
            },
            reaped,
        )
    }

    /// Kills the group now, whatever has happened to its leader.
    pub(crate) fn kill(&self) {
        kill_process_group(self.pgid);
    }
}

impl Drop for TurnGroup {
    fn drop(&mut self) {
        // A reaped leader's id may have been given to another process: never signal it then.
        if !self.reaped.load(Ordering::SeqCst) {
            kill_process_group(self.pgid);
        }
        running_groups().remove(&self.pgid);
    }
}

/// Registers a process group a turn spawned elsewhere (a conversation's CLI) as running, so
/// [`kill_running_turns`] reaches it; unregistered when the guard drops.
pub struct RunningGroup(u32);

impl RunningGroup {
    pub fn new(pgid: u32) -> Self {
        running_groups().insert(pgid);
        Self(pgid)
    }
}

impl Drop for RunningGroup {
    fn drop(&mut self) {
        running_groups().remove(&self.0);
    }
}

/// Kills every process group a running turn leads: what a Ctrl+C in the TUI does. Returns whether
/// any turn was running.
pub fn kill_running_turns() -> bool {
    let groups: Vec<u32> = running_groups().iter().copied().collect();
    for pgid in &groups {
        kill_process_group(*pgid);
    }
    !groups.is_empty()
}

/// SIGKILLs every process in the group `pgid`.
#[cfg(unix)]
pub(crate) fn kill_process_group(pgid: u32) {
    // SAFETY: `killpg` takes a process-group id and a signal and touches no memory. A group that
    // has already gone makes the call fail with ESRCH, which is logged and otherwise ignored.
    let rc = unsafe { libc::killpg(pgid as libc::pid_t, libc::SIGKILL) };
    if rc != 0 {
        log::debug!(
            "[headless] killpg({pgid}) failed: {}",
            io::Error::last_os_error()
        );
    }
}

#[cfg(not(unix))]
pub(crate) fn kill_process_group(_pgid: u32) {}

/// `duration` as a deadline is named in an error: `4h`, `30m`, `1s`, `250ms`.
pub(crate) fn describe_deadline(duration: Duration) -> String {
    let secs = duration.as_secs();
    if duration.subsec_nanos() != 0 {
        format!("{}ms", duration.as_millis())
    } else if secs != 0 && secs.is_multiple_of(3600) {
        format!("{}h", secs / 3600)
    } else if secs != 0 && secs.is_multiple_of(60) {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}
