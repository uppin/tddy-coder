//! Ending a tmux-run session's agent, not only its tmux session (`#tab-hardening` 2/4).
//!
//! `tmux kill-session` sends the pane's process SIGHUP and closes its terminal, and an agent that
//! ignores SIGHUP and does not read its terminal survives both. So a delete reads the pane's pid
//! and when that process started, kills the tmux session, then SIGTERMs the pid and SIGKILLs it
//! after a grace — each only while the pid still names a process that started when the agent did.
//! When the session recorded the agent's start (`SessionMetadata::pid_start_time`), that is the
//! agent's start: a pane process whose start differs is not the agent the session started, and is
//! never signalled.

use std::time::Duration;

use tddy_cli_sessions::process_start::ProcessStart;
use tddy_cli_sessions::tmux::TmuxError;

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
    let _ = (os_user, name, recorded, grace);
    // TODO(tab-hygiene): implement
    todo!()
}
