use crate::cli_agent_steps::ManagedWiring;
use crate::harness_from_spec::CliSessionType;
use crate::project_home_launch::StartRole;
use crate::session_acting_identity::SessionAccountAccess;
use crate::spec_launch::HostAgent;
use crate::work_queue_parent::SpawnWorkQueueParent;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::provider_route::RouteSources;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_session_split::service_util::OrchestratorLink;

/// Everything one CLI harness start reads.
pub(crate) struct CliSessionStart<'a> {
    /// The CLI session type the session runs, with the spec that launches it.
    pub kind: CliSessionType,
    /// The coding agent the session launches, resolved on this host before anything is cut.
    pub agent: &'a HostAgent,
    pub config: &'a DaemonConfig,
    pub tddy_data_dir: &'a Path,
    /// The session's OS user's agent-tool socket, resolved before anything is cut
    /// ([`LaunchSessions::agent_tool_socket_of`]).
    pub agent_tool_socket: &'a Path,
    pub cli_manager: &'a Arc<CliSessionManager>,
    pub os_user: &'a str,
    pub session_id: &'a str,
    /// Authorizes the `workspace` starts this session's seeded agents need on their own hosts.
    pub session_token: &'a str,
    pub sessions_base: PathBuf,
    /// The model the operator chose; empty leaves the choice to the harness.
    pub model: &'a str,
    pub project_id: &'a str,
    pub repo_path: &'a str,
    pub branch_worktree_intent: &'a str,
    pub new_branch_name: &'a str,
    pub selected_integration_base_ref: &'a str,
    pub selected_branch_to_work_on: &'a str,
    pub work_queue_parent: SpawnWorkQueueParent<'a>,
    /// The orchestrator that started the session, recorded in its first changeset. Absent, the
    /// stack parent (if any) is the orchestrator recorded.
    pub orchestrator: Option<OrchestratorLink>,
    /// The orchestrator the request named, on this host, with its agent when known: the session is
    /// launched under its native peer name and told how to report back (`#orchestra` 2/9).
    pub reports_to: Option<crate::back_channel::ReportsTo<'a>>,
    pub initial_prompt: &'a str,
    /// The session's starting agent roster, already resolved by the caller.
    pub agents: &'a mut [tddy_core::SessionAgentRecord],
    pub semantic_index: bool,
    pub create_remote_branch: bool,
    pub task_registry: &'a tddy_task::TaskRegistry,
    pub agent_clones: &'a dyn tddy_session_agents::seed_codebase::SeededAgentClones,
    /// What the start reads its owner's vault with: the provider account and the commit identity.
    pub account_access: &'a SessionAccountAccess,
    pub route_sources: &'a RouteSources,
    pub progress: &'a AttachmentProgressSink,
    /// Run the harness inside a tmux session as the session's OS user
    /// (`StartSessionRequest.run_in_tmux`, `#tmux-agent` 3/3).
    pub run_in_tmux: bool,
    /// `StartSessionRequest.permission_mode`; blank leaves the spec's own default.
    pub permission_mode: &'a str,
    pub dangerously_skip_permissions: bool,
    /// `StartSessionRequest.effort`: the level the agent's spec carries into its launch; blank
    /// leaves the agent's own default.
    pub effort: &'a str,
    /// The ssh `Host` alias the checkout lives on; empty for a checkout on this host.
    pub ssh_config_host: &'a str,
    /// The target the session's agent runs on (`StartSessionRequest.target`), when it named one:
    /// the checkout is cut on it and the agent is launched there.
    pub target: Option<tddy_core::SessionTarget>,
    /// Who the start's caller is, as their session token resolved: the owner of the vault a
    /// Coder target's account is read from.
    pub subject: String,
    /// Reads a Coder target's account from the caller's vault.
    pub coder_credentials: Arc<dyn tddy_host_service::coder_workspaces::CoderCredentialSource>,
    /// Where a target session's followed hooks are delivered, and the registry its follower is
    /// kept in (`LaunchSessions`' fields, lent for the start).
    pub hook_delivery: Option<Arc<dyn crate::remote_hook_delivery::HookReportDelivery>>,
    pub hook_followers: crate::remote_hook_follower::RemoteHookFollowers,
    /// Whether the session was started over a managed codebase; recorded on the session.
    pub managed_codebase: bool,
    /// The managed workflow the session is driven by, for a spec declaring `managed_workflow`.
    pub managed: Option<ManagedWiring>,
    /// The session's standing role, recorded before its agent is spawned.
    pub role: StartRole<'a>,
    /// The agent credential the session tool transport is minted around — `None` exports nothing:
    /// the daemon's own callerless children, a session whose agent runs on another host, a daemon
    /// that signs no tokens.
    pub session_tool_token: Option<String>,
}

/// The shell that reaches `target`, resolved as `subject`: an ssh target's transport, or a
/// Coder workspace's with the credentials `credentials` reads from the subject's vault. A
/// locked vault is `FAILED_PRECONDITION` with its own remedy, as a resume must answer.
pub(crate) fn shell_for_session_target(
    target: &tddy_core::SessionTarget,
    subject: &str,
    credentials: &dyn tddy_host_service::coder_workspaces::CoderCredentialSource,
) -> Result<Arc<dyn tddy_remote_shell::RemoteShell>, Status> {
    use crate::coder_target::CoderTargetError;
    match target {
        tddy_core::SessionTarget::Ssh { .. } => {
            crate::remote_agent_launch::shell_for_target(target)
                .map_err(|e| Status::invalid_argument(e.to_string()))
        }
        tddy_core::SessionTarget::Coder { .. } => {
            crate::coder_target::shell_for_coder_target(target, subject, credentials).map_err(|e| {
                match e {
                    CoderTargetError::VaultLocked | CoderTargetError::Unavailable(_) => {
                        Status::failed_precondition(e.to_string())
                    }
                    other => Status::invalid_argument(other.to_string()),
                }
            })
        }
    }
}
