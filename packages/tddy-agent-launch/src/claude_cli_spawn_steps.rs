use tddy_session_split::service_util;

use uuid::Uuid;

use tddy_rpc::Status;

use std::path::Path;

use tddy_coding_agent_specs::{Capability, CodingAgentSpec};
use tddy_core::SessionMetadata;
use tddy_daemon_kernel::config::DaemonConfig;

use tddy_daemon_kernel::daemon_hook_urls;

/// The lifecycle hooks a claude-cli start wires its agent to: the relay that reports them, where it
/// reports, and the per-session token the daemon checks.
pub(super) struct ClaudeCliHooks {
    pub(super) tddy_tools_path: String,
    daemon_url: String,
    daemon_socket: Option<String>,
    pub(super) hook_token: String,
    pub(super) answer_wait_secs: u64,
    /// The hook log of a session whose agent runs on a target: every hook appends there instead
    /// of running `tddy-tools session-hook`. Set by the harness launch when the session has a
    /// target; `None` is a session on this host.
    pub(super) remote_log: Option<String>,
    /// The provider route's non-secret environment pairs, pinned into the hooks settings file
    /// ([`tddy_core::HookCommandParams::route_env`]); empty for an unrouted launch.
    pub(super) route_env: Vec<(String, String)>,
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
            remote_log: self.remote_log.as_deref(),
            route_env: &self.route_env,
        }
    }

    /// Point every hook at the hook log of a session whose agent runs on a target, its worktree
    /// there being `worktree`: each hook appends to that log instead of running
    /// `tddy-tools session-hook`. The log is named by its absolute path
    /// ([`tddy_core::remote_hook_log_in`]), so a hook finds it whatever directory the agent has
    /// entered since it started.
    pub(super) fn point_at_the_target_log(&mut self, worktree: &Path) {
        self.remote_log = Some(tddy_core::remote_hook_log_in(worktree));
    }
}

/// A claude-cli start's hooks, with a fresh per-session token.
///
/// `agent_tool_socket` is the session's OS user's agent-tool socket, served by this daemon
/// (`tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets::ensure_bound`): the hooks report
/// over it on every host.
///
/// Nothing is read from a daemon.yaml agent section (`#agent-tools 19/19`): `tddy_tools_path` is the
/// agent's resolved `tddy-tools` dependency, the url is the daemon's own (`listen.advertise_url`),
/// and the answer wait is the agent spec's `coding_agent.answer_wait_secs` — which the launch
/// records beside the session ([`record_hook_answer_wait`]) so the server's hold matches it.
pub(super) fn claude_cli_hooks(
    config: &DaemonConfig,
    agent_tool_socket: &Path,
    spec: &CodingAgentSpec,
    tddy_tools_path: &str,
) -> ClaudeCliHooks {
    claude_cli_hooks_with_token(
        config,
        agent_tool_socket,
        spec,
        tddy_tools_path,
        Uuid::new_v4().to_string(),
    )
}

/// The hooks a resume renders again, rebuilt around the start's recorded token — every adapter's,
/// whether it delivers them on argv (Muse's `-c hooks=…`) or in a file (Claude's per-session
/// `--settings` file). A resume that kept a start's file would keep whatever daemon socket that
/// start named, and a daemon restart moves the socket (reported 2026-11-21). A session that recorded
/// no token (started before its agent declared hooks) resumes hookless, as before.
pub(super) fn resumed_hooks(
    spec: &CodingAgentSpec,
    config: &DaemonConfig,
    agent_tool_socket: &Path,
    tddy_tools_path: &str,
    meta: &SessionMetadata,
) -> Result<Option<ClaudeCliHooks>, Status> {
    if !spec.has(Capability::Hooks) {
        return Ok(None);
    }
    let Some(hook_token) = meta.hook_token.clone() else {
        return Ok(None);
    };
    Ok(Some(claude_cli_hooks_with_token(
        config,
        agent_tool_socket,
        spec,
        tddy_tools_path,
        hook_token,
    )))
}

/// A resume's hooks: the start's wiring rebuilt around the recorded `hook_token`.
pub(super) fn claude_cli_hooks_with_token(
    config: &DaemonConfig,
    agent_tool_socket: &Path,
    spec: &CodingAgentSpec,
    tddy_tools_path: &str,
    hook_token: String,
) -> ClaudeCliHooks {
    ClaudeCliHooks {
        tddy_tools_path: tddy_tools_path.to_string(),
        daemon_url: daemon_hook_urls::advertise_daemon_url(config),
        daemon_socket: Some(agent_tool_socket.to_string_lossy().into_owned()),
        hook_token,
        answer_wait_secs: spec.answer_wait_secs(),
        remote_log: None,
        route_env: Vec::new(),
    }
}

/// Record the wait `hooks` were written with beside the session in `session_dir`, so
/// `AwaitQuestionAnswer` holds the agent's hook for as long as the agent was told to wait.
pub(crate) fn record_hook_answer_wait(
    session_dir: &Path,
    hooks: &ClaudeCliHooks,
) -> Result<(), Status> {
    record_answer_wait_secs(session_dir, hooks.answer_wait_secs)
}

/// Record a hook wait of `answer_wait_secs` beside the session in `session_dir`.
pub(crate) fn record_answer_wait_secs(
    session_dir: &Path,
    answer_wait_secs: u64,
) -> Result<(), Status> {
    tddy_session_activity::answer_wait::record_answer_wait(
        session_dir,
        std::time::Duration::from_secs(answer_wait_secs),
    )
    .map_err(|e| Status::internal(format!("failed to record the answer wait: {e}")))
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

#[cfg(test)]
mod target_hook_log_tests;
