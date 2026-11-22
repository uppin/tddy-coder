//! Ending a tmux-run session's agent, not only its tmux session (`#tab-hardening` 2/4).
//!
//! `tmux kill-session` sends the pane's process SIGHUP and closes its terminal, and an agent that
//! ignores SIGHUP and does not read its terminal survives both. So a delete reads the pane's pid
//! and when that process started, kills the tmux session, then SIGTERMs the pid and SIGKILLs it
//! after a grace — each only while the pid still names a process that started when the agent did.
//! When the session recorded the agent's start on its tmux main's roster node
//! ([`tddy_core::tab_roster::START_PROPERTY`]), that is the agent's start: a pane process whose start differs is not the agent the session started, and is
//! never signalled.

use std::time::Duration;

use tddy_cli_sessions::process_start::{process_start, ProcessStart};
use tddy_cli_sessions::tmux::{kill_tmux_session, pane_pid_and_start, TmuxError};

/// How often the agent is checked for having ended during the grace.
const POLL: Duration = Duration::from_millis(100);

/// How long the agent of a deleted tmux session gets after SIGTERM before it is killed — the grace
/// a tool session's stop gives its child.
pub(crate) const AGENT_END_GRACE: Duration = Duration::from_secs(5);

/// Kill `os_user`'s tmux session `name`, and end the agent its pane ran: SIGTERM, then SIGKILL once
/// `grace` has passed, each only while the pane's pid names a process that started at `recorded`
/// (the session's record), or — for a record that kept no start — at the start read before the
/// kill. Returns once that process is gone or no longer that process.
///
/// A tmux session that is already gone is not an error, and leaves nothing to signal. A tmux that
/// refuses the kill is the [`TmuxError`].
pub(crate) async fn end_tmux_run_agent(
    os_user: &str,
    name: &str,
    recorded: Option<ProcessStart>,
    grace: Duration,
) -> Result<(), TmuxError> {
    let pane = {
        let (user, session) = (os_user.to_string(), name.to_string());
        tokio::task::spawn_blocking(move || pane_pid_and_start(&user, &session))
            .await
            .map_err(|e| TmuxError::new(format!("the pane read did not finish: {e}")))?
    };
    kill_tmux_session_once(os_user, name).await?;
    match pane {
        Ok((pid, read_before_the_kill)) => {
            let agent_start = recorded.unwrap_or(read_before_the_kill);
            end_process_started_at(pid, agent_start, grace).await;
        }
        // No pane to read: the session is gone, or has no process — nothing to signal.
        Err(error) => log::debug!("tmux session {name:?} has no pane process to end: {error}"),
    }
    Ok(())
}

async fn kill_tmux_session_once(os_user: &str, name: &str) -> Result<(), TmuxError> {
    let (user, session) = (os_user.to_string(), name.to_string());
    tokio::task::spawn_blocking(move || kill_tmux_session(&user, &session))
        .await
        .map_err(|e| TmuxError::new(format!("the tmux kill did not finish: {e}")))?
}

/// SIGTERM `pid`, then SIGKILL it once `grace` has passed, each only while it is still the process
/// that started at `start`.
async fn end_process_started_at(pid: u32, start: ProcessStart, grace: Duration) {
    if !signal_if_still(pid, start, libc::SIGTERM) {
        return;
    }
    let deadline = tokio::time::Instant::now() + grace;
    while tokio::time::Instant::now() < deadline {
        if process_start(pid) != Some(start) {
            return;
        }
        tokio::time::sleep(POLL).await;
    }
    if signal_if_still(pid, start, libc::SIGKILL) {
        log::warn!(
            "the agent pid={pid} of a deleted tmux session outlived its grace period; killed it"
        );
    }
}

/// Send `signal` to `pid` when it still names the process that started at `start`; whether it did.
fn signal_if_still(pid: u32, start: ProcessStart, signal: i32) -> bool {
    if process_start(pid) != Some(start) {
        return false;
    }
    // SAFETY: `kill` takes two integers and touches no memory of ours. The start just matched, so
    // the pid names the agent the session started.
    if unsafe { libc::kill(pid as libc::pid_t, signal) } != 0 {
        log::warn!(
            "kill({pid}, {signal}) of a deleted tmux session's agent failed: {}",
            std::io::Error::last_os_error()
        );
        return false;
    }
    true
}
