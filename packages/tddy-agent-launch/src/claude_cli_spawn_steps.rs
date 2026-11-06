use tddy_session_split::service_util;

use uuid::Uuid;

use tddy_rpc::Status;

use std::path::Path;

use tddy_daemon_kernel::config::DaemonConfig;

use tddy_daemon_kernel::daemon_hook_urls;

/// The lifecycle hooks a claude-cli start wires its agent to: the relay that reports them, where it
/// reports, and the per-session token the daemon checks.
pub(super) struct ClaudeCliHooks {
    pub(super) tddy_tools_path: String,
    daemon_url: String,
    daemon_socket: Option<String>,
    pub(super) hook_token: String,
    answer_wait_secs: u64,
}

impl ClaudeCliHooks {
    /// The hooks' command parameters for `session_id`, run by `os_user`.
    pub(super) fn params<'a>(
        &'a self,
        session_id: &'a str,
        os_user: &'a str,
    ) -> tddy_core::HookCommandParams<'a> {
        tddy_core::HookCommandParams {
            tddy_tools_path: &self.tddy_tools_path,
            daemon_url: &self.daemon_url,
            session_id,
            os_user,
            hook_token: &self.hook_token,
            daemon_socket: self.daemon_socket.as_deref(),
            answer_wait_secs: self.answer_wait_secs,
        }
    }
}

/// A claude-cli start's hooks, with a fresh per-session token.
///
/// `tddy_data_dir` is the data dir the daemon *resolved* — `TDDY_DATA_DIR` included, which a
/// configuration file need not agree with (`./desktop-dev` sets it absolute beside a relative
/// `tmp/.tddy`) — because an embedded daemon's agent-tool socket path is digested from it.
pub(super) fn claude_cli_hooks(config: &DaemonConfig, tddy_data_dir: &Path) -> ClaudeCliHooks {
    let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
        config
            .claude_cli
            .as_ref()
            .and_then(|c| c.tddy_tools_path.as_deref()),
    );
    ClaudeCliHooks {
        tddy_tools_path,
        daemon_url: daemon_hook_urls::claude_hook_daemon_url(config),
        // An embedded daemon serves no HTTP RPC; its hooks report over this socket.
        daemon_socket: tddy_daemon_kernel::embedded_host_agent_tool_socket(tddy_data_dir),
        hook_token: Uuid::new_v4().to_string(),
        answer_wait_secs: config.claude_cli_answer_wait_secs(),
    }
}

/// The notify wiring a claude-cli start and resume apply — see [`crate::notify_env`].
pub use crate::notify_env::{
    agent_notify_launch, resumed_agent_notify_launch, AgentNotifyLaunch, AgentNotifyWiring,
};

/// Record a session's standing role in its `.session.yaml` before its agent is spawned, so there is
/// no moment in which the agent runs and the daemon reads the session as ordinary. A session without
/// a role has nothing to record early. `session_type` is the record's.
pub(super) fn record_role_of_session_type_before_spawn(
    session_dir: &Path,
    session_id: &str,
    project_id: &str,
    session_role: &str,
    session_type: &str,
    worktree_path: &Path,
    managed_codebase: bool,
) -> Result<(), Status> {
    if session_role.is_empty() {
        return Ok(());
    }
    let meta = tddy_core::SessionMetadata {
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        managed_codebase,
        session_role: Some(session_role.to_string()),
        ..service_util::starting_session_metadata(session_id, project_id, session_type)
    };
    tddy_core::write_session_metadata(session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to record the session's role: {e}")))
}

#[cfg(test)]
mod claude_cli_spawn_steps_tests;
