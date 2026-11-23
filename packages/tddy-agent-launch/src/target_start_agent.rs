//! The agent of a session started or resumed on a **target** (`#tab-hardening` 4/4): the target's
//! own, never this host's. Its spec is read from the installed coding agents with its **bare
//! command** as its binary — what the target's login `PATH` resolves — and nothing of the agent is
//! resolved on this host, so a daemon host with no copy of it starts a session on a target that has
//! one. Before anything of the start exists, the target is asked for that command (the probe of
//! `#tab-hardening` 1/4), so a target without the agent fails the start naming it, with no worktree
//! cut and no session recorded. The tools the daemon runs itself — the `tddy-tools` closure its
//! hooks and relays name — stay resolved on this host.
//!
//! A target agent tab already works this way (`opencode_cli_spawn::restore_tabs::new_target_tab`,
//! `#session-liveness` 8/8); this is the session's own start and resume.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-target-start-agent.md

use std::path::Path;
use std::sync::Arc;

use tddy_remote_shell::RemoteShell;
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionRequest;

use crate::harness_from_spec::{resolve_session_agent, CliSessionAgent, CliSessionType};
use crate::launch_ports::LaunchSessions;
use crate::remote_agent_launch::{probe_agent_on_target, tab_launch_status};
use crate::spec_launch::HostCodingAgents;

/// The coding agent a CLI session of `kind` runs on a target, read from `agents` without resolving
/// it on this host: its spec, and a tool whose `command` **and** `binary` are the spec's bare
/// command (`claude`, `agent`) — what the target's login `PATH` resolves and the launch on the
/// target runs — never a path of this host, even when this host has the agent installed. The
/// agent's deps (`tddy-tools`) are resolved on this host as [`resolve_session_agent`] resolves
/// them: the daemon runs them itself.
///
/// `FAILED_PRECONDITION` for an agent the installed coding agents do not register, and for a dep
/// this host does not hold, naming it.
pub(crate) fn resolve_session_agent_for_target(
    agents: &Arc<HostCodingAgents>,
    kind: CliSessionType,
) -> Result<CliSessionAgent, Status> {
    let agents = Arc::clone(agents);
    let agent = agents
        .resolve_for_target(kind.spec_id)
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
    Ok(CliSessionAgent {
        kind,
        agent,
        agents,
    })
}

/// Ask the target `shell` reaches whether its login shell finds `session_agent`'s bare command —
/// `probe_agent_on_target` (`#tab-hardening` 1/4), one call through the port, run from the login's
/// own directory (`.`): a start has no worktree on the target yet, and a resume whose worktree is
/// gone there must be told so by its launch, not as a missing agent.
///
/// Its refusal says what a target tab's says (`tab_launch_status`): `FAILED_PRECONDITION` naming
/// the command for an agent the target does not have, `UNAVAILABLE` with the transport's own reason
/// for a target that cannot be reached.
pub(crate) async fn require_the_agent_on_the_target(
    shell: &dyn RemoteShell,
    session_agent: &CliSessionAgent,
) -> Result<(), Status> {
    probe_agent_on_target(
        shell,
        Path::new("."),
        &session_agent.agent.tool.binary.to_string_lossy(),
    )
    .await
    .map_err(|e| tab_launch_status(&e))
}

/// Ask the target `shell` reaches whether its login shell finds `tmux`, for a start or resume that
/// asks for a tmux run there ([`crate::remote_agent_launch::probe_tmux_on_target`], from the
/// login's own directory). A target without tmux is refused `FAILED_PRECONDITION` naming tmux —
/// never run without it — and an unreachable one `UNAVAILABLE`.
pub(crate) async fn require_tmux_on_the_target(shell: &dyn RemoteShell) -> Result<(), Status> {
    crate::remote_agent_launch::probe_tmux_on_target(shell, Path::new("."))
        .await
        .map_err(|e| tab_launch_status(&e))
}

impl LaunchSessions {
    /// The coding agent a CLI session of `kind` runs, settled where it runs: with no `target`,
    /// resolved on this host ([`resolve_session_agent`]); on the target `target` reaches, read from
    /// its spec without a host binary ([`resolve_session_agent_for_target`]) and then asked of the
    /// target ([`require_the_agent_on_the_target`]) — and, for a tmux run there, tmux too
    /// ([`require_tmux_on_the_target`]). A start settles it before its prelude and its
    /// worktree cut, a resume before it re-wires anything.
    pub(crate) async fn agent_to_launch(
        &self,
        kind: CliSessionType,
        target: &Option<Arc<dyn RemoteShell>>,
        in_tmux: bool,
    ) -> Result<CliSessionAgent, Status> {
        let Some(shell) = target else {
            return resolve_session_agent(&self.coding_agents, kind);
        };
        let session_agent = resolve_session_agent_for_target(&self.coding_agents, kind)?;
        require_the_agent_on_the_target(&**shell, &session_agent).await?;
        if in_tmux {
            require_tmux_on_the_target(&**shell).await?;
        }
        Ok(session_agent)
    }

    /// The shell the target `req` names is reached through, resolved as the caller before anything
    /// of the start exists; `None` for a start on this host. A target that cannot be resolved — a
    /// malformed one, a Coder account whose vault is locked or does not hold it — refuses the start
    /// here.
    pub(crate) fn start_target_shell(
        &self,
        req: &StartSessionRequest,
    ) -> Result<Option<Arc<dyn RemoteShell>>, Status> {
        let target = crate::remote_agent_launch::session_target_of_proto(req.target.as_ref())
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let Some(target) = target else {
            return Ok(None);
        };
        crate::opencode_cli_spawn::session_start::shell_for_session_target(
            &target,
            &(self.user_resolver)(&req.session_token).unwrap_or_default(),
            self.coder_credentials.as_ref(),
        )
        .map(Some)
    }
}

#[cfg(test)]
#[path = "target_start_agent_tests.rs"]
mod tests;
