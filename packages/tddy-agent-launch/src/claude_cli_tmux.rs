//! The tmux main terminal every CLI agent shares (`#tmux-agent` 1/3,
//! docs/ft/daemon/claude-cli-session.md § tmux mode): the handle a launch returns, the blocking-pool
//! helpers, the liveness probe and the lazy re-attach after a daemon restart.
//!
//! The agent runs in a tmux session of its own, as the session's OS user; the session's main
//! terminal is a tmux client attached to it, and the pid the session records is the pane's process —
//! so liveness, signal and delete act on the agent, not on the client.

use std::path::Path;
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_cli_sessions::tmux::{agent_tmux_session_name, kill_tmux_session, tmux_session_alive};
use tddy_rpc::Status;

/// A CLI agent running in tmux, and the main terminal attached to it.
pub struct TmuxLaunched {
    /// The main terminal: a tmux client of [`Self::tmux_session`].
    pub handle: Arc<PtyHandle>,
    /// The pid of the agent's pane process — what the session records as its `pid`.
    pub pane_pid: u32,
    pub tmux_session: String,
}

/// Run a blocking tmux operation off the async executor. Every tmux call spawns a process and
/// waits for it, so none of them may run on the executor's threads.
pub(crate) async fn in_tmux_blocking<T, F>(operation: F) -> Result<T, Status>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|e| Status::internal(format!("tmux task did not finish: {e}")))
}

/// Kill the tmux session of a start that failed after it began. The start's own error is what the
/// caller is told, so a kill that fails is logged rather than returned.
pub(crate) async fn stop_tmux_session_of_failed_start(os_user: &str, name: &str) {
    let (os_user, name) = (os_user.to_string(), name.to_string());
    let outcome = tokio::task::spawn_blocking({
        let name = name.clone();
        move || kill_tmux_session(&os_user, &name)
    })
    .await;
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::warn!(
            target: "tddy_daemon::claude_cli_session",
            "could not stop tmux session {name:?} of a start that failed: {error}"
        ),
        Err(error) => log::warn!(
            target: "tddy_daemon::claude_cli_session",
            "the stop of tmux session {name:?} of a start that failed did not finish: {error}"
        ),
    }
}

/// Whether the tmux session of `session_id`'s agent is alive for `os_user`, probed on the blocking
/// pool.
pub async fn agent_tmux_session_is_alive(os_user: &str, session_id: &str) -> Result<bool, Status> {
    let (os_user, name) = (os_user.to_string(), agent_tmux_session_name(session_id));
    in_tmux_blocking(move || tmux_session_alive(&os_user, &name)).await
}

/// Re-attach `session_id`'s main terminal from what its session directory records, when the session
/// ran in tmux and its tmux session is alive — the lazy re-attach a first stream triggers after a
/// daemon restart or a client's exit — through [`crate::cli_harness_tmux::reattach_recorded_harness`],
/// and register its main recording as a start registers it. `false` when the record is unreadable,
/// the session did not run in tmux, its tmux session is gone, its agent has exited or the attach
/// failed (logged); nothing is relaunched.
///
/// Which agent the record is re-attached as is read from `agents` — the coding agents this daemon
/// installed, the source every start and resume reads — through its recorded session type alone
/// (`#agent-tools 17/19`): a record without one is skipped with a warning, never re-attached as a
/// default agent, and a host with no coding agents (`None`) re-attaches nothing (logged as the
/// `FAILED_PRECONDITION` a start would be refused with). `tddy_data_dir` is only where the session's
/// record lives.
pub async fn reattach_recorded_main(
    manager: &CliSessionManager,
    agents: Option<&crate::spec_launch::HostCodingAgents>,
    tddy_data_dir: &Path,
    os_user: &str,
    session_id: &str,
) -> bool {
    use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
    if validate_session_id_segment(session_id).is_err() {
        return false;
    }
    let session_dir = unified_session_dir_path(tddy_data_dir, session_id);
    let Ok(meta) = tddy_core::read_session_metadata(&session_dir) else {
        return false;
    };
    if meta.tmux_session.is_none() {
        return false;
    }
    match agent_tmux_session_is_alive(os_user, session_id).await {
        Ok(true) => {}
        Ok(false) => return false,
        Err(error) => {
            log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "could not tell whether the tmux session of {session_id} is alive: {}",
                error.message()
            );
            return false;
        }
    }
    let worktree_path =
        tddy_core::repo_root_for_session(&session_dir).unwrap_or_else(|| session_dir.clone());
    let kind = match crate::harness_from_spec::recorded_cli_session_type(
        agents,
        meta.session_type.as_deref(),
    ) {
        Ok(Some(kind)) => kind,
        Ok(None) => {
            log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "the record of {session_id} names no CLI agent's session type: its main terminal is not re-attached"
            );
            return false;
        }
        Err(error) => {
            log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "could not re-attach the main terminal of {session_id}: {}",
                error.message()
            );
            return false;
        }
    };
    let recorded = crate::cli_harness_tmux::RecordedHarness {
        os_user,
        session_id,
        session_type: kind.name,
        worktree_path,
        session_dir: &session_dir,
    };
    match crate::cli_harness_tmux::reattach_recorded_harness(manager, agents, recorded).await {
        Ok(true) => {
            crate::claude_cli_spawn::record_main_terminal(manager, tddy_data_dir, session_id).await;
            true
        }
        Ok(false) => false,
        Err(error) => {
            log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "could not re-attach the main terminal of {session_id}: {}",
                error.message()
            );
            false
        }
    }
}
