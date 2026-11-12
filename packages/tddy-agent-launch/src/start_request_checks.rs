use crate::svc_ensure_project_available_for_start::StartHandles;
use std::path::Path;

use tddy_core::{write_session_github_source, SessionGithubSource};

use crate::stack_seed_validation::validate_stack_seed_base_session;

use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_session_split::service_util;

use tddy_coding_agent_specs::{Capability, CodingAgentSpec, LaunchKind};
use tddy_rpc::Status;

use tddy_service::proto::session::{BranchConflict, StartSessionResponse};

use tddy_service::proto::session::{GithubSource, StartSessionRequest};

use crate::launch_ports::LaunchSessions;
use crate::start_keepalive::{with_keepalive, START_KEEPALIVE_INTERVAL};
use tddy_projects::project_storage::ProjectData;
use tddy_session_files::attachment_progress::AttachmentProgressSink;

/// The request a start is forwarded to a peer with: a request that names no GitHub source names
/// this host's own project setting (`local_row`), so the peer acts the same way even if its copy of
/// the project was never set. A request that already names one keeps it; with no local row there is
/// nothing to name.
pub fn forwarded_start_request(
    req: &StartSessionRequest,
    local_row: Option<&ProjectData>,
) -> StartSessionRequest {
    let mut forwarded = req.clone();
    if req.github_source() == GithubSource::Unspecified {
        if let Some(row) = local_row {
            forwarded.set_github_source(if row.local_gh {
                GithubSource::LocalGh
            } else {
                GithubSource::Vault
            });
        }
    }
    forwarded
}
/// Record the GitHub source a start named in the new session's directory, so its resumes act the
/// same way; a start that named none records nothing, and its resumes read this host's row again.
pub(crate) fn record_requested_github_source(
    req: &StartSessionRequest,
    session_dir: &Path,
) -> Result<(), Status> {
    let source = match req.github_source() {
        GithubSource::Unspecified => return Ok(()),
        GithubSource::Vault => SessionGithubSource::Vault,
        GithubSource::LocalGh => SessionGithubSource::LocalGh,
    };
    std::fs::create_dir_all(session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
    write_session_github_source(session_dir, source)
        .map_err(|e| Status::internal(format!("failed to record the session's GitHub source: {e}")))
}

/// A start of a CLI coding agent that cannot be served is refused before anything is created: a
/// managed codebase is refused, `INVALID_ARGUMENT` naming the agent, for a spec that does not declare
/// `managed_workflow` (`#agent-tools 14/19`). A sandboxed start's placement is the spec's own refusal
/// (`LaunchSessions::require_sandbox_placement`), checked by the caller.
pub(crate) fn refuse_an_unsupported_cli_session_start(
    spec: &CodingAgentSpec,
    session_type: &str,
    req: &StartSessionRequest,
) -> Result<(), Status> {
    if req.managed_codebase && !spec.has(Capability::ManagedWorkflow) {
        return Err(Status::invalid_argument(format!(
            "managed_codebase is not supported for {session_type} sessions: {} declares no \
             managed_workflow",
            spec.id
        )));
    }
    Ok(())
}

/// Whether `session_type` is a CLI agent's — declared with an `agent_cli` launch — which runs its
/// agent in tmux when asked. Any other type ignores `run_in_tmux`.
fn runs_in_tmux(session_type: &str, agents: Option<&crate::spec_launch::HostCodingAgents>) -> bool {
    agents
        .and_then(|agents| agents.agents().for_session_type(session_type))
        .is_some()
}

/// A start that asks for a tmux-run agent together with a placement that is not a plain agent on
/// this host is refused before anything is created, for every CLI agent's session type, naming the
/// conflict:
///
/// - `sandbox` confines the agent in its own jail, which a tmux session on the host is not;
/// - `sandboxed_codebase` withdraws the agent's native tools and jails the code, so there is no host
///   agent to run in tmux;
/// - `codebase_daemon_instance_id` (split placement) puts the worktree on another daemon, while the
///   tmux session is as this host's OS user in a local worktree.
pub(crate) fn refuse_a_conflicting_tmux_run_start(
    req: &StartSessionRequest,
    agents: Option<&crate::spec_launch::HostCodingAgents>,
) -> Result<(), Status> {
    if !req.run_in_tmux || !runs_in_tmux(req.session_type.trim(), agents) {
        return Ok(());
    }
    if req.sandbox {
        return Err(Status::invalid_argument(
            "run_in_tmux cannot be combined with sandbox: a sandboxed agent runs in its own jail, \
             not in a tmux session on the host",
        ));
    }
    if req.sandboxed_codebase {
        return Err(Status::invalid_argument(
            "run_in_tmux cannot be combined with sandboxed_codebase: that placement withdraws the \
             agent's native tools and jails the code, so there is no agent on the host to run in tmux",
        ));
    }
    let codebase_host = req.codebase_daemon_instance_id.trim();
    if !codebase_host.is_empty() {
        return Err(Status::invalid_argument(format!(
            "run_in_tmux cannot be combined with codebase_daemon_instance_id {codebase_host:?}: a \
             split session's worktree is on another daemon, and a tmux session runs in a worktree \
             on this host"
        )));
    }
    Ok(())
}

/// An agent that declares `requires_model` always needs a model: a start naming its session type
/// without one is refused before anything is created.
pub(crate) fn refuse_a_cli_start_without_a_model(
    req: &StartSessionRequest,
    agents: Option<&crate::spec_launch::HostCodingAgents>,
) -> Result<(), Status> {
    let session_type = req.session_type.trim();
    let requires_model = agents
        .and_then(|agents| agents.agents().for_session_type(session_type))
        .is_some_and(|spec| spec.has(tddy_coding_agent_specs::Capability::RequiresModel));
    if requires_model && req.model.trim().is_empty() {
        return Err(Status::invalid_argument(format!(
            "model is required for {session_type} sessions"
        )));
    }
    Ok(())
}

/// A start naming an effort its agent cannot take is refused before anything is created
/// (`#orchestrators` 3/11), `INVALID_ARGUMENT`: a level the agent's spec does not declare, naming
/// the agent and its levels, and any level for an agent whose spec declares none, naming the agent.
/// An empty effort (the agent's own default) is admitted for every agent.
pub fn refuse_an_effort_the_agent_cannot_take(
    _req: &StartSessionRequest,
    _agents: Option<&crate::spec_launch::HostCodingAgents>,
) -> Result<(), Status> {
    // TODO(start-effort): implement — read the session type's spec's `effort` slot:
    // `effort "turbo" is not one of claude's effort levels (low, medium, high, xhigh, max)` and
    // `effort is not supported for opencode-cli sessions: opencode declares no effort levels`.
    Ok(())
}

/// Refuse an orchestrator on a session whose type is declared with the `tddy_coder` launch: its
/// changeset is written by its own `tddy-coder` process, not by this daemon, so the link could not be
/// recorded.
///
/// TODO(start-parity): pass the orchestrator to the spawned `tddy-coder`, which records it the way
/// it records `--stack-parent`.
pub(crate) fn refuse_an_orchestrated_tool_start(
    req: &StartSessionRequest,
    agents: &crate::spec_launch::HostCodingAgents,
) -> Result<(), Status> {
    let session_type = req.session_type.trim();
    let launches_tddy_coder = agents
        .agents()
        .session_type(session_type)
        .is_some_and(|declared| declared.launch == LaunchKind::TddyCoder);
    if launches_tddy_coder && !req.orchestrator_session_id.trim().is_empty() {
        return Err(Status::failed_precondition(format!(
            "a {session_type:?} session cannot be started with an orchestrator yet: its changeset \
             is written by its own tddy-coder process, which does not record one"
        )));
    }
    Ok(())
}

/// Refuse a `tddy_coder` start whose `tool_path` is not one of the daemon's `allowed_tools`, an
/// empty one included: the path is executed, so only a binary the operator listed may be, and an
/// empty one names nothing (it would resolve to the daemon's own working directory). There is no
/// default tool: the caller picks one of the listed paths.
pub(crate) fn refuse_a_tool_path_the_daemon_does_not_allow(
    req: &StartSessionRequest,
    config: &tddy_daemon_kernel::config::DaemonConfig,
) -> Result<(), Status> {
    let tool_path = req.tool_path.trim();
    let allowed: Vec<&str> = config
        .allowed_tools()
        .iter()
        .map(|tool| tool.path.as_str())
        .collect();
    if !tool_path.is_empty() && allowed.contains(&tool_path) {
        return Ok(());
    }
    let refused = if tool_path.is_empty() {
        format!(
            "tool_path is required for a {:?} session",
            req.session_type.trim()
        )
    } else {
        format!("tool_path {tool_path:?} is not in allowed_tools")
    };
    Err(Status::invalid_argument(format!(
        "{refused}; this daemon's allowed_tools are [{}]",
        allowed
            .iter()
            .map(|path| format!("{path:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;

impl LaunchSessions {
    /// The refusals a start the caller is about to begin meets before it creates anything: a session
    /// type this host does not declare, a `tddy_coder` start whose `tool_path` is not allowed, a CLI
    /// session without a model, a `tddy_coder` session with an orchestrator, and a requested branch another
    /// session owns (`Some`, naming the owner). The start core makes the same checks once its task
    /// has named itself, which is too late for the caller to be told by refusal, so a caller that
    /// answers with a handle asks these first (`#start-parity` 4/9).
    pub async fn refuse_a_start_before_it_begins(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
    ) -> Result<Option<BranchConflict>, Status> {
        let launch = crate::harness_from_spec::requested_session_launch(
            &self.coding_agents,
            &req.session_type,
        )?;
        if launch == crate::harness_from_spec::SessionLaunch::TddyCoder {
            refuse_a_tool_path_the_daemon_does_not_allow(req, &self.config)?;
        }
        refuse_a_cli_start_without_a_model(req, Some(&*self.coding_agents))?;
        refuse_an_orchestrated_tool_start(req, &self.coding_agents)?;
        self.owned_branch_conflict(os_user, req).await
    }

    /// Signal the task `task_id` this host runs, as a `CancelTask` does; whether it is registered
    /// here (`#start-parity` 4/9).
    pub async fn cancel_local_task(&self, task_id: &str) -> bool {
        self.task_registry
            .cancel_task(&tddy_task::TaskId(task_id.to_string()))
            .await
    }

    pub(super) fn eligible_daemon_ids(&self) -> (String, Vec<String>) {
        let local_id = local_instance_id_for_config(&self.config);
        let eligible_rows = self
            .peer_routing
            .eligible_daemon_source()
            .list_eligible_daemons();
        let eligible_ids: Vec<String> = eligible_rows
            .iter()
            .map(|e| e.instance_id.0.clone())
            .collect();
        (local_id, eligible_ids)
    }

    pub(super) async fn forward_start_session(
        &self,
        req: &StartSessionRequest,
        peer_instance_id: String,
    ) -> Result<StartSessionResponse, Status> {
        log::info!(
            "StartSession: forwarding RPC to remote daemon_instance_id={}",
            peer_instance_id
        );
        let slot = self.peer_routing.common_room_livekit_room().ok_or_else(|| {
                Status::failed_precondition(
                    "cannot forward StartSession: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)",
                )
            })?;
        // Relayed into a discarding sink: the unary caller has no stream to report into, and the
        // forward is bounded by the peer's keep-alives and the relay's idle timeout, not a deadline.
        let discarded = AttachmentProgressSink::discarding();
        let inner = tddy_daemon_kernel::peer_calls::forward_start_session_relaying(
            slot,
            &peer_instance_id,
            req,
            &|event| discarded.relay(event),
        )
        .await
        .map_err(tddy_daemon_kernel::peer_calls::ForwardedStartFailure::into_status)?;
        log::info!(
            "StartSession: forward succeeded session_id={} livekit_server_identity={}",
            inner.session_id,
            inner.livekit_server_identity
        );
        Ok(inner)
    }

    /// Forward a start to a peer, naming the GitHub source this host's own row of the project gives
    /// it: the peer may hold no (or a stale) copy of the project. No row here: nothing to name.
    pub(super) async fn forward_start_naming_local_source(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        peer_instance_id: String,
    ) -> Result<StartSessionResponse, Status> {
        let local_row = service_util::find_registered_project(
            &self.tddy_data_dir,
            os_user,
            req.project_id.trim(),
        )
        .ok()
        .map(|(_, project)| project);
        let forwarded = forwarded_start_request(req, local_row.as_ref());
        self.forward_start_session(&forwarded, peer_instance_id)
            .await
    }

    pub(super) async fn provision_project_for_start(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        progress: &AttachmentProgressSink,
        cancel: &tddy_task::CancellationToken,
    ) -> Result<(), Status> {
        let project_id = req.project_id.trim();
        if !project_id.is_empty() {
            let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve projects path"))?;
            // A clone can wait on the operator's passphrase far longer than a forwarded stream
            // tolerates silence, so the stream is kept alive for as long as this waits.
            with_keepalive(
                progress,
                START_KEEPALIVE_INTERVAL,
                self.ensure_project_available_for_start(
                    os_user,
                    &projects_dir,
                    project_id,
                    &req.session_token,
                    req.agent_clone.as_ref(),
                    StartHandles { progress, cancel },
                ),
            )
            .await?;
        }
        Ok(())
    }

    pub(super) fn validate_stack_seed_against_project(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        sessions_base: std::path::PathBuf,
        project_id: &str,
    ) -> Result<(), Status> {
        let (_, project) =
            service_util::find_registered_project(&self.tddy_data_dir, os_user, project_id)?;
        validate_stack_seed_base_session(
            &sessions_base,
            &req.recipe,
            &req.pr_stack_base_session_id,
            Path::new(&project.main_repo_path),
        )
        .map_err(tddy_service::to_rpc_status)?;
        Ok(())
    }
}

#[cfg(test)]
mod managed_workflow_tests {
    use pretty_assertions::assert_eq;
    use tddy_rpc::Code;

    use super::*;

    fn a_managed_start() -> StartSessionRequest {
        StartSessionRequest {
            managed_codebase: true,
            ..Default::default()
        }
    }

    fn the_builtin(id: &str) -> CodingAgentSpec {
        tddy_coding_agent_specs::embedded_spec(id).expect("the spec is built in")
    }

    #[test]
    fn a_managed_start_of_an_agent_declaring_managed_workflow_is_admitted() {
        // When
        let admitted = (
            refuse_an_unsupported_cli_session_start(
                &the_builtin("claude"),
                "claude-cli",
                &a_managed_start(),
            )
            .map_err(|status| status.message),
            refuse_an_unsupported_cli_session_start(
                &the_builtin("cursor"),
                "cursor-cli",
                &a_managed_start(),
            )
            .map_err(|status| status.message),
        );

        // Then
        assert_eq!(admitted, (Ok(()), Ok(())));
    }

    #[test]
    fn a_managed_start_of_an_agent_without_managed_workflow_is_refused_naming_the_agent() {
        // When
        let refused = refuse_an_unsupported_cli_session_start(
            &the_builtin("opencode"),
            "opencode-cli",
            &a_managed_start(),
        )
        .map_err(|status| (status.code, status.message));

        // Then
        assert_eq!(
            refused,
            Err((
                Code::InvalidArgument,
                "managed_codebase is not supported for opencode-cli sessions: opencode declares no \
                 managed_workflow"
                    .to_string()
            ))
        );
    }

    #[test]
    fn an_unmanaged_start_of_any_agent_is_admitted() {
        // When
        let admitted = refuse_an_unsupported_cli_session_start(
            &the_builtin("opencode"),
            "opencode-cli",
            &StartSessionRequest::default(),
        )
        .map_err(|status| status.message);

        // Then
        assert_eq!(admitted, Ok(()));
    }
}
