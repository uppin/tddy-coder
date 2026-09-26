//! The one way this engine starts a shell command.
//!
//! Every tool-call shell — the blocking `Shell` path, a background `ShellTaskBody`, and
//! [`crate::LocalShell::run`] — goes through [`run_contained`], because a shell started any other
//! way inherits two defects that together cost a session:
//!
//! * `tokio::process::Command::output()` sets stdout and stderr but, unlike its `std` counterpart,
//!   leaves **stdin inherited**. Inside a jail that stdin is `tddy-sandbox-runner --stdio`'s
//!   tool-IPC request pipe, so a command that reads standard input becomes a second reader on the
//!   daemon→jail channel and consumes frames meant for the runner.
//! * `tokio::time::timeout` only stops waiting. The command keeps running — and keeps reading —
//!   long after the caller has been told it timed out.
//!
//! A contained command therefore gets `/dev/null` for standard input, its own process group, and
//! — when it overruns its budget — a signal to that whole **group**, since the descendants of a
//! command like `( sleep 1; touch marker ) & wait` outlive the `sh` the engine started.

use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;

/// How long a timed-out process group is given to honour `SIGTERM` before it is killed. The same
/// escalation `tddy_daemon_sandbox::sandbox_session::terminate_sandbox_process` applies to a jail
/// leader; the crates cannot depend on each other, so the pattern is repeated rather than shared.
#[cfg(unix)]
const TERMINATION_GRACE: Duration = Duration::from_millis(200);

/// Why a contained shell command produced no output.
#[derive(Debug)]
pub(crate) enum ContainedShellError {
    /// The command could not be started, or could not be waited on.
    Io(std::io::Error),
    /// The command outlived its budget. Its process group has been signalled, so nothing it
    /// started is still running by the time this is returned.
    TimedOut(Duration),
}

impl std::fmt::Display for ContainedShellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "spawn failed: {e}"),
            Self::TimedOut(budget) => write!(f, "timed out after {}ms", budget.as_millis()),
        }
    }
}

/// Run `command` under `sh -c` in `root`, with `env` added to the inherited environment.
///
/// The command cannot read the caller's standard input, and with `Some(budget)` it cannot outlive
/// that budget: on overrun its whole process group is signalled before the error is returned.
/// `None` runs it to completion — the background-job case, which has no budget to exceed.
pub(crate) async fn run_contained(
    command: &str,
    root: &Path,
    env: &[(String, String)],
    budget: Option<Duration>,
) -> Result<Output, ContainedShellError> {
    let mut spawn = tokio::process::Command::new("sh");
    spawn
        .arg("-c")
        .arg(command)
        .current_dir(root)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        // Never the caller's: in a jail that stream is the tool-IPC request pipe.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A dropped future must not leave the child behind; its descendants are the group's job.
        .kill_on_drop(true);
    in_its_own_process_group(&mut spawn);

    let child = spawn.spawn().map_err(ContainedShellError::Io)?;
    let leader = child.id();

    let Some(budget) = budget else {
        return child
            .wait_with_output()
            .await
            .map_err(ContainedShellError::Io);
    };

    let running = child.wait_with_output();
    tokio::pin!(running);
    match tokio::time::timeout(budget, &mut running).await {
        Ok(finished) => finished.map_err(ContainedShellError::Io),
        Err(_) => {
            // `running` still owns the child, so it has not been reaped and its process group id
            // is still in use — every descendant remains addressable as `-leader`.
            terminate_process_group(leader).await;
            Err(ContainedShellError::TimedOut(budget))
        }
    }
}

/// Make the child its own process-group leader, so a timeout can signal the group rather than
/// only the `sh` the engine started. A group of 0 means "use the child's own pid".
#[cfg(unix)]
fn in_its_own_process_group(spawn: &mut tokio::process::Command) {
    spawn.process_group(0);
}

// TODO: a non-unix build has no process-group containment, so a timed-out command's descendants
// survive on `kill_on_drop` of the direct child alone. Nothing this crate runs on is non-unix
// today; give it a job object before that changes.
#[cfg(not(unix))]
fn in_its_own_process_group(_spawn: &mut tokio::process::Command) {}

/// `SIGTERM` the group `leader` heads, then `SIGKILL` whatever is still there.
#[cfg(unix)]
async fn terminate_process_group(leader: Option<u32>) {
    // No pid means the child already exited, so there is no group left to signal.
    let Some(leader) = leader else { return };
    let leader = leader as i32;

    // SAFETY: `leader` is this process's own un-reaped child, so neither its pid nor the group it
    // leads can have been recycled onto an unrelated process.
    unsafe { libc::kill(-leader, libc::SIGTERM) };
    tokio::time::sleep(TERMINATION_GRACE).await;
    if unsafe { libc::kill(leader, 0) } == 0 {
        unsafe {
            libc::kill(leader, libc::SIGKILL);
            libc::kill(-leader, libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
async fn terminate_process_group(_leader: Option<u32>) {}
