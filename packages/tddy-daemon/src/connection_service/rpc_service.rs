// The parent module's own imports, carried in: this block was the whole `impl
// ConnectionServiceTrait` and reaches the same traits and helpers it always did. Unused
// entries are pruned below by the compiler's own spans.
// `encode_to_vec` is a `prost::Message` method; the trait is imported anonymously because
// only its methods are used.
use crate::tool_engine;
use prost::Message as _;
use tddy_sandbox_runner::ExecuteToolResponse;
use tddy_service::proto::connection::start_session_event::Event as StartSessionEventKind;
use tddy_service::proto::connection::ConnectionService as ConnectionServiceTrait;
use tddy_service::proto::connection::SessionEntry as ProtoSessionEntry;
use tddy_service::proto::connection::{
    AcpReplayFrame, AddPlannedPrRequest, AddPlannedPrResponse, GetAcpReplayPageRequest,
    GetAcpReplayPageResponse, GetAcpToolCallDetailRequest, GetAcpToolCallDetailResponse,
    GetPrStatusRequest, GetPrStatusResponse, GetWorktreeSnapshotRequest,
    GetWorktreeSnapshotResponse, LinkStackNodeRequest, LinkStackNodeResponse,
    MintLocalTokenRequest, MintLocalTokenResponse, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReorderPlannedPrRequest,
    ReorderPlannedPrResponse, RepointPlannedPrRequest, RepointPlannedPrResponse,
    ResolveStackBaseRequest, ResolveStackBaseResponse,
    SessionNotificationEvent as ProtoSessionNotificationEvent, StartSessionEvent,
    StreamAcpReplayRequest,
};
use tddy_service::proto::connection::{
    AgentActivityDeltaChunk, AgentActivityDeltaRequest, ExecuteToolRequest,
    ProjectEntry as ProtoProjectEntry,
};
use tddy_service::proto::connection::{
    AgentActivityRecord as ProtoAgentActivityRecord, StreamSessionNotificationsRequest,
};
use tddy_service::proto::connection::{
    DemoVmState, GetDemoVmStatusRequest, GetDemoVmStatusResponse, ReportAgentActivityRequest,
    ReportAgentActivityResponse, ReportSessionStatusRequest, ReportSessionStatusResponse,
    StartDemoVmRequest, StartDemoVmResponse, StopDemoVmRequest, StopDemoVmResponse,
    StreamSessionActivityRequest, ToolCallInfo as ProtoToolCallInfo,
};
use tddy_service::proto::connection::{
    ExecuteToolChunk, ListExecToolsRequest, ListExecToolsResponse, ListSessionToolCallsRequest,
    ListSessionToolCallsResponse,
};

use crate::{
    connection_service::{activity_hub, agent_roster, hooks_and_urls, service_util},
    project_storage, session_deletion, session_list_enrichment, session_reader,
};
use tddy_spawn::{spawn_worker, spawner};

use tddy_service::proto::activity::ActivityService as _;
use tddy_service::proto::session_agents_svc::SessionAgentService as _;

use super::svc_old_coordinate_shim::{
    activity_record_at_the_old_coordinate, relayed_onto_this_coordinate,
    roster_at_the_old_coordinate,
};

use super::base_sync_unavailable;

use super::base_sync_view;

use super::owner_repo_from_repo_root;

use super::worktree_leg;

use super::require_pr_stack_orchestrator;

use super::exec_tool_result_frames;

use super::reject_exec_tool_path_traversal;

use tddy_service::proto::connection::ListProjectBranchesResponse;

use tddy_service::proto::connection::ListProjectBranchesRequest;

use tddy_service::proto::connection::DeleteSessionResponse;

use tddy_service::proto::connection::DeleteSessionRequest;

use tddy_service::proto::connection::SignalSessionResponse;

use tddy_service::proto::connection::SignalSessionRequest;

use tddy_spawn::spawner::SpawnOptions;

use tddy_service::proto::connection::ResumeSessionResponse;

use tddy_service::proto::connection::ResumeSessionRequest;

use tddy_core::read_session_metadata;

use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_core::session_lifecycle::validate_session_id_segment;

use tddy_service::proto::connection::ConnectSessionResponse;

use tddy_service::proto::connection::ConnectSessionRequest;

use super::AttachmentProgressSink;

use tddy_service::proto::connection::StartSessionResponse;

use tddy_service::proto::connection::StartSessionRequest;

use tddy_service::proto::connection::SetProjectDefaultBranchResponse;

use tddy_service::proto::connection::SetProjectDefaultBranchRequest;

use crate::livekit_peer_discovery::PeerRoute;

use tddy_service::proto::connection::AddProjectToHostResponse;

use tddy_service::proto::connection::AddProjectToHostRequest;

use crate::project_storage::ProjectData;

use crate::user_sessions_path::repos_base_for_user;

use crate::user_sessions_path::project_path_under_home_from_user_relative;

use tddy_service::proto::connection::CreateProjectResponse;

use tddy_service::proto::connection::CreateProjectRequest;

use super::merge_listed_projects_with_peers;

use crate::user_sessions_path::projects_path_for_user;

use tddy_service::proto::connection::ListProjectsResponse;

use tddy_service::proto::connection::ListProjectsRequest;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_service::proto::connection::ListSessionsResponse;

use tddy_service::proto::connection::ListSessionsRequest;

use tddy_service::proto::connection::CancelAgentConversationResponse;

use tddy_service::proto::connection::CancelAgentConversationRequest;

use tddy_service::proto::connection::PromptAgentConversationRequest;

use tddy_service::proto::connection::AgentConversationChunk;

use std::sync::Arc;

use uuid::Uuid;

use tddy_service::proto::connection::OpenAgentConversationResponse;

use tddy_service::proto::connection::OpenAgentConversationRequest;

use tddy_service::proto::connection::StreamSessionAgentsRequest;

use super::MpscResultStream;

use tddy_service::proto::connection::ListSessionAgentsRequest;

use tddy_service::proto::connection::DetachSessionAgentRequest;

use tddy_service::proto::connection::SessionAgentRoster;

use tddy_service::proto::connection::AttachSessionAgentRequest;

use tddy_service::proto::connection::SubagentInfo;

use crate::livekit_peer_discovery::local_instance_id_for_config;

use tddy_service::proto::connection::ListSubagentsResponse;

use tddy_service::proto::connection::ListSubagentsRequest;

use super::parse_agent_models_json;

use super::list_models_probe_args;

use std::path::PathBuf;

use std::path::Path;

use super::AGENT_MODELS_CACHE_TTL;

use super::agent_models_cache;

use tddy_service::proto::connection::ListAgentModelsResponse;

use tddy_service::proto::connection::ListAgentModelsRequest;

use crate::agent_list_mapping::agent_allowlist_rows;

use tddy_service::proto::connection::AgentInfo;

use tddy_service::proto::connection::ListAgentsResponse;

use tddy_service::proto::connection::ListAgentsRequest;

use tddy_service::proto::connection::ToolInfo;

use tddy_rpc::Status;

use tddy_service::proto::connection::ListToolsResponse;

use tddy_rpc::Response;

use tddy_service::proto::connection::ListToolsRequest;

use tddy_rpc::Request;

use super::ConnectionServiceImpl;

#[async_trait::async_trait]
impl ConnectionServiceTrait for ConnectionServiceImpl {
    async fn list_tools(
        &self,
        _request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        self.record_rpc_activity();
        let tools: Vec<ToolInfo> = self
            .config
            .allowed_tools()
            .iter()
            .map(|t| {
                let label = t
                    .label
                    .as_deref()
                    .and_then(tddy_daemon_kernel::trim_to_option)
                    .unwrap_or_else(|| t.path.clone());
                ToolInfo {
                    path: t.path.clone(),
                    label,
                }
            })
            .collect();
        Ok(Response::new(ListToolsResponse { tools }))
    }

    async fn list_agents(
        &self,
        _request: Request<ListAgentsRequest>,
    ) -> Result<Response<ListAgentsResponse>, Status> {
        log::debug!("list_agents RPC: mapping config allowlist to AgentInfo");
        // A registry this daemon has but cannot read is an error, not "there are no assistants" —
        // a session started against a missing agent id fails much later and much less clearly.
        let assistants = match &self.model_registry {
            Some(registry) => registry
                .list_assistants()
                .await
                .map_err(tddy_rpc::Status::from)?,
            None => Vec::new(),
        };
        let agents: Vec<AgentInfo> = agent_allowlist_rows(&self.config, &assistants)
            .into_iter()
            .map(|row| AgentInfo {
                id: row.id,
                label: row.display_label,
            })
            .collect();
        log::info!("list_agents RPC: returning {} agent(s)", agents.len());
        Ok(Response::new(ListAgentsResponse { agents }))
    }

    /// Enumerate the models an agent supports by shelling out to `tddy-tools list-models` as the
    /// caller's OS user. Results are cached per (agent, daemon) for a short TTL. A failed probe is
    /// surfaced as an RPC error — never masked with a fallback catalog.
    ///
    /// Runs the probe on the local daemon; `daemon_instance_id` participates only in the cache key
    /// (cross-daemon forwarding is not wired here — the web fetches models from the daemon it is
    /// already connected to).
    async fn list_agent_models(
        &self,
        request: Request<ListAgentModelsRequest>,
    ) -> Result<Response<ListAgentModelsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?
            .to_string();

        let agent = req.agent.trim().to_string();
        if agent.is_empty() {
            return Err(Status::invalid_argument("agent is required"));
        }

        // Key by OS user: cursor / ACP catalogs (and the "current, default" model) are
        // account-specific, so one user's list must never be served to another from the cache.
        let cache_key = format!(
            "{}\u{1f}{}\u{1f}{}",
            os_user,
            req.daemon_instance_id.trim(),
            agent
        );
        if let Ok(cache) = agent_models_cache().lock() {
            if let Some((cached_at, resp)) = cache.get(&cache_key) {
                if cached_at.elapsed() < AGENT_MODELS_CACHE_TTL {
                    return Ok(Response::new(resp.clone()));
                }
            }
        }

        let tools_path = self.resolve_tddy_tools_path();
        // Cursor's model probe must hand tddy-tools the resolved absolute `agent` path (as the PTY
        // spawn does), so the impersonated child execs a fully-qualified binary instead of doing a
        // PATH lookup that lacks the install dir. Only forward an absolute path — a bare-name
        // resolution keeps the existing behavior (no `--cursor-cli-path`).
        let cursor_cli_path = (agent == "cursor")
            .then(|| crate::config::resolve_cursor_binary_path(&self.config))
            .filter(|p| std::path::Path::new(p).is_absolute())
            .map(std::path::PathBuf::from);
        let probe_args = list_models_probe_args(&agent, cursor_cli_path.as_deref());
        let probe = tokio::task::spawn_blocking(move || {
            spawner::run_capture_as_user(&os_user, &tools_path, &probe_args)
        })
        .await
        .map_err(|e| Status::internal(format!("model probe join error: {e}")))?
        .map_err(|e| Status::failed_precondition(format!("model probe failed: {e}")))?;

        let resp = parse_agent_models_json(&probe)?;

        if let Ok(mut cache) = agent_models_cache().lock() {
            cache.insert(cache_key, (std::time::Instant::now(), resp.clone()));
        }
        Ok(Response::new(resp))
    }

    /// Resolved specialized-agent defs available to wire into a managed-codebase session — every
    /// source a name can resolve against here, so `<tddyhome>/agents/*.yaml` (see
    /// docs/ft/coder/specialized-subagents.md) *and* this daemon's registry assistants.
    ///
    /// Answered from [`Self::resolvable_agent_defs`], which is also what an attach resolves the id
    /// it is handed against: what a picker is offered and what it can then attach are one list, not
    /// two that can drift. Advertising less than that is what made an assistant created in Models &
    /// Agents invisible to the roster while being perfectly attachable by name.
    ///
    /// Every row is stamped with this daemon's instance id and the qualified `agent_id` it is
    /// attached by. A picker fans this call out across every common-room daemon, and two of them
    /// routinely answer with a def called `explorer`: without the stamp the merged list cannot say
    /// which host offers which row, and the id the picker sends would be a guess rather than the
    /// one the serving daemon minted.
    ///
    /// A def whose own name contains `@` is dropped with a warning: its qualified id would parse
    /// back as a different pair, so advertising it would hand a picker an id that routes elsewhere.
    async fn list_subagents(
        &self,
        _request: Request<ListSubagentsRequest>,
    ) -> Result<Response<ListSubagentsResponse>, Status> {
        log::debug!("list_subagents RPC: resolving agent defs");
        let daemon_instance_id = local_instance_id_for_config(&self.config);
        let defs = self.resolvable_agent_defs().await?;
        let resolved = defs.len();
        let subagents: Vec<SubagentInfo> = defs
            .into_iter()
            .filter_map(
                |def| match agent_roster::subagent_info(&def, &daemon_instance_id) {
                    Ok(info) => Some(info),
                    Err(e) => {
                        log::warn!("list_subagents RPC: not advertising a def — {e}");
                        None
                    }
                },
            )
            .collect();
        // An empty answer has three very different causes — this daemon has no defs, its registry
        // was never wired in, or a def was dropped on the way out — and "returning 0" told them
        // apart in none of them. Naming the sources is what makes an empty picker diagnosable from
        // the log alone, on a host whose filesystem is not to hand.
        log::info!(
            "list_subagents RPC: returning {} subagent(s) of {} resolved def(s) [agents dir {}, \
             model registry {}]: {:?}",
            subagents.len(),
            resolved,
            self.tddy_data_dir.join("agents").display(),
            if self.model_registry.is_some() {
                "attached"
            } else {
                "absent"
            },
            subagents.iter().map(|s| &s.agent_id).collect::<Vec<_>>()
        );
        Ok(Response::new(ListSubagentsResponse { subagents }))
    }

    // ── Session agent roster (docs/ft/daemon/session-agent-roster.md) ─────────────────────────

    /// Attach one agent to a live session, or report the roster unchanged when it is already there.
    ///
    /// The order is the contract: the caller is authenticated, then the id is resolved, then the
    /// session is checked for being able to enforce what the agent withdraws, then the roster is
    /// written. Every step before the write is one that must not happen for a caller who turns out
    /// not to be allowed — resolving a remote id contacts a peer and provisions a checkout on it
    /// (PRD AC12).
    /// Family B moved to `session_agents.SessionAgentService` in `#unbundle` node 7. This
    /// coordinate keeps answering all nine by delegating to that surface — the routed one, so a
    /// request naming another daemon is served there exactly as it was here.
    ///
    /// Every one of the nine below is this shape: re-address the request to the new coordinate,
    /// hand it to [`ConnectionServiceImpl::session_agents_service`], and re-address the answer
    /// back. The two messages are field-for-field identical, so the re-addressing is a relabelling
    /// rather than a mapping; it exists only because the two coordinates are two generated types.
    ///
    /// TODO(session-agent-services): the whole block goes when this coordinate stops declaring
    /// these nine rpcs, which is the next milestone. Nothing new should be added to it.
    async fn attach_session_agent(
        &self,
        request: Request<AttachSessionAgentRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        let req = request.into_inner();
        let roster = self
            .session_agents_surface()
            .attach_session_agent(Request::new(
                tddy_service::proto::session_agents_svc::AttachSessionAgentRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    agent_id: req.agent_id,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(roster_at_the_old_coordinate(roster)))
    }

    async fn detach_session_agent(
        &self,
        request: Request<DetachSessionAgentRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        let req = request.into_inner();
        let roster = self
            .session_agents_surface()
            .detach_session_agent(Request::new(
                tddy_service::proto::session_agents_svc::DetachSessionAgentRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    agent_id: req.agent_id,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(roster_at_the_old_coordinate(roster)))
    }

    async fn list_session_agents(
        &self,
        request: Request<ListSessionAgentsRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        let req = request.into_inner();
        let roster = self
            .session_agents_surface()
            .list_session_agents(Request::new(
                tddy_service::proto::session_agents_svc::ListSessionAgentsRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(roster_at_the_old_coordinate(roster)))
    }

    type StreamSessionAgentsStream = MpscResultStream<SessionAgentRoster>;

    async fn stream_session_agents(
        &self,
        request: Request<StreamSessionAgentsRequest>,
    ) -> Result<Response<Self::StreamSessionAgentsStream>, Status> {
        let req = request.into_inner();
        let frames = self
            .session_agents_surface()
            .stream_session_agents(Request::new(
                tddy_service::proto::session_agents_svc::StreamSessionAgentsRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(relayed_onto_this_coordinate(
            frames,
            roster_at_the_old_coordinate,
        )))
    }

    async fn open_agent_conversation(
        &self,
        request: Request<OpenAgentConversationRequest>,
    ) -> Result<Response<OpenAgentConversationResponse>, Status> {
        let req = request.into_inner();
        let opened = self
            .session_agents_surface()
            .open_agent_conversation(Request::new(
                tddy_service::proto::session_agents_svc::OpenAgentConversationRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    agent_id: req.agent_id,
                    conversation_id: req.conversation_id,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(OpenAgentConversationResponse {
            conversation_id: opened.conversation_id,
        }))
    }

    type PromptAgentConversationStream = MpscResultStream<AgentConversationChunk>;

    async fn prompt_agent_conversation(
        &self,
        request: Request<PromptAgentConversationRequest>,
    ) -> Result<Response<Self::PromptAgentConversationStream>, Status> {
        let req = request.into_inner();
        let frames = self
            .session_agents_surface()
            .prompt_agent_conversation(Request::new(
                tddy_service::proto::session_agents_svc::PromptAgentConversationRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    conversation_id: req.conversation_id,
                    prompt: req.prompt,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(relayed_onto_this_coordinate(
            frames,
            |chunk: tddy_service::proto::session_agents_svc::AgentConversationChunk| {
                AgentConversationChunk {
                    content_chunk: chunk.content_chunk,
                    stop_reason: chunk.stop_reason,
                    last: chunk.last,
                }
            },
        )))
    }

    async fn cancel_agent_conversation(
        &self,
        request: Request<CancelAgentConversationRequest>,
    ) -> Result<Response<CancelAgentConversationResponse>, Status> {
        let req = request.into_inner();
        self.session_agents_surface()
            .cancel_agent_conversation(Request::new(
                tddy_service::proto::session_agents_svc::CancelAgentConversationRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    conversation_id: req.conversation_id,
                },
            ))
            .await?;
        Ok(Response::new(CancelAgentConversationResponse {}))
    }

    async fn report_agent_clone_state(
        &self,
        request: Request<tddy_service::proto::connection::ReportAgentCloneStateRequest>,
    ) -> Result<Response<tddy_service::proto::connection::ReportAgentCloneStateResponse>, Status>
    {
        let req = request.into_inner();
        self.session_agents_surface()
            .report_agent_clone_state(Request::new(
                tddy_service::proto::session_agents_svc::ReportAgentCloneStateRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    codebase_session_id: req.codebase_session_id,
                    clone_state: req.clone_state,
                    clone_error: req.clone_error,
                    worktree_path: req.worktree_path,
                    divergences: req.divergences,
                },
            ))
            .await?;
        Ok(Response::new(
            tddy_service::proto::connection::ReportAgentCloneStateResponse {},
        ))
    }

    async fn report_agent_conversation_state(
        &self,
        request: Request<tddy_service::proto::connection::ReportAgentConversationStateRequest>,
    ) -> Result<
        Response<tddy_service::proto::connection::ReportAgentConversationStateResponse>,
        Status,
    > {
        let req = request.into_inner();
        self.session_agents_surface()
            .report_agent_conversation_state(Request::new(
                tddy_service::proto::session_agents_svc::ReportAgentConversationStateRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    agent_id: req.agent_id,
                    status: req.status,
                    summary: req.summary,
                },
            ))
            .await?;
        Ok(Response::new(
            tddy_service::proto::connection::ReportAgentConversationStateResponse {},
        ))
    }

    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let timeout = self.config.spawn_worker_request_timeout();
        let sessions_base_blocking = sessions_base.clone();
        let local_daemon_id = local_instance_id_for_config(&self.config);
        // Tailing a session reads its transcript once, on the listing that first sees it, so the
        // seed is paid for on this blocking thread rather than on the reactor.
        let session_agent_inference = Arc::clone(&self.session_agent_inference);
        let agent_activity_hub = Arc::clone(&self.agent_activity_hub);
        let entries = service_util::spawn_blocking_with_timeout(
            timeout,
            "ListSessions: read and enrich",
            move || {
                let sessions = session_reader::list_sessions_in_dir(&sessions_base_blocking)
                    .map_err(|e| anyhow::anyhow!(e))?;
                let mut out = Vec::with_capacity(sessions.len());
                for s in sessions {
                    let session_dir = sessions_base_blocking
                        .join(SESSIONS_SUBDIR)
                        .join(&s.session_id);
                    let mut entry = ProtoSessionEntry {
                        session_id: s.session_id,
                        created_at: s.created_at,
                        status: s.status,
                        repo_path: s.repo_path,
                        pid: s.pid.unwrap_or(0),
                        is_active: s.is_active,
                        project_id: s.project_id,
                        daemon_instance_id: local_daemon_id.clone(),
                        workflow_goal: String::new(),
                        workflow_state: String::new(),
                        elapsed_display: String::new(),
                        agent: String::new(),
                        model: String::new(),
                        pending_elicitation: false,
                        activity_status: String::new(),
                        tool: s.tool,
                        session_type: s.session_type,
                        updated_at: s.updated_at,
                        livekit_room: s.livekit_room,
                        previous_session_id: s.previous_session_id,
                        orchestrator_session_id: String::new(),
                        recipe: String::new(),
                        stack_plan_json: String::new(),
                        // FIXME(2026-07-12-fast-session-change): populate from a per-session
                        // traffic meter for GrpcSessionTerminal sessions the daemon owns.
                        // Zero/empty is the honest value until that meter is wired; tddy-coder
                        // sessions report live counters via the participant runtime instead.
                        bytes_in: 0,
                        bytes_out: 0,
                        last_data_received_at: String::new(),
                        // Populated by `apply_session_list_status_to_proto` below from the recipe
                        // manifest; left empty here so the enrichment is the single source of truth.
                        context_docs: Vec::new(),
                        // Populated by `apply_session_list_status_to_proto` below from
                        // Changeset.branch; left empty here so the enrichment is the single source
                        // of truth.
                        branch: String::new(),
                        // A split session's pairing, straight from `.session.yaml`. Empty for a
                        // co-located session, which is every session that does not name another
                        // daemon as its codebase host.
                        codebase_daemon_instance_id: s.codebase_daemon_instance_id,
                        codebase_session_id: s.codebase_session_id,
                        // Inferred below from the session's own conversation
                        // (docs/ft/daemon/agent-session-status.md). UNSPECIFIED with no activity is
                        // the honest value for a session nothing has been observed on, and stays the
                        // value for every session type that runs no agent.
                        agent_status:
                            tddy_service::proto::connection::SessionAgentStatus::Unspecified as i32,
                        last_activity: None,
                    };
                    if let Err(e) = session_list_enrichment::apply_session_list_status_to_proto(
                        &session_dir,
                        &mut entry,
                    ) {
                        log::warn!(
                            target: "tddy_daemon::connection_service",
                            "ListSessions: enrichment failed for {}: {}",
                            session_dir.display(),
                            e
                        );
                    }
                    // Only a session that runs an agent is tailed — the same gate
                    // `report_session_status` applies, and for the same reason: those are the two
                    // session types that write a conversation. A `workspace` session holds a clone
                    // and would spend a subscription and a file read to conclude UNSPECIFIED.
                    if matches!(entry.session_type.as_str(), "claude-cli" | "cursor-cli") {
                        session_agent_inference.ensure_tailing(
                            &agent_activity_hub,
                            &entry.session_id,
                            &session_dir,
                        );
                        let inferred = crate::session_agent_inference::inferred_activity(
                            tddy_core::SessionActivityStatus::from_wire(&entry.activity_status),
                            session_agent_inference.latest(&entry.session_id).as_ref(),
                        );
                        entry.agent_status =
                            crate::session_agent_inference::session_agent_status(inferred.as_ref())
                                as i32;
                        entry.last_activity = inferred
                            .as_ref()
                            .and_then(crate::session_agent_status::AgentActivity::to_proto);
                    }
                    out.push(entry);
                }
                Ok(out)
            },
        )
        .await?;
        Ok(Response::new(ListSessionsResponse { sessions: entries }))
    }

    async fn list_projects(
        &self,
        request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let projects = project_storage::read_projects(&projects_dir)
            .map_err(|e| Status::internal(e.to_string()))?;
        let local_daemon_id = local_instance_id_for_config(&self.config);
        let entries: Vec<ProtoProjectEntry> = projects
            .into_iter()
            .map(|p| {
                let repo_root = PathBuf::from(&p.main_repo_path);
                let default_remote = hooks_and_urls::resolve_default_remote_or_empty(
                    &projects_dir,
                    &p.project_id,
                    &repo_root,
                );
                hooks_and_urls::project_entry_from(&p, local_daemon_id.clone(), default_remote)
            })
            .collect();
        log::debug!(
            target: "tddy_daemon::connection_service",
            "list_projects: local_registry_rows={} local_daemon_instance_id={}",
            entries.len(),
            local_daemon_id
        );
        // `local_only` returns just this daemon's rows and skips peer fan-out, breaking the
        // recursion when a peer aggregation call fans out back into `ListProjects`.
        let projects = if req.local_only {
            entries
        } else {
            merge_listed_projects_with_peers(
                &*self.eligible_daemon_source,
                &req.session_token,
                entries,
            )
            .await
        };
        Ok(Response::new(ListProjectsResponse { projects }))
    }

    async fn create_project(
        &self,
        request: Request<CreateProjectRequest>,
    ) -> Result<Response<CreateProjectResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let name = req.name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("project name is required"));
        }
        if name.contains('/') || name.contains("..") {
            return Err(Status::invalid_argument("invalid project name"));
        }
        let git_url = req.git_url.trim();
        if git_url.is_empty() {
            return Err(Status::invalid_argument("git_url is required"));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;

        let user_rel = req.user_relative_path.trim();
        let destination = if !user_rel.is_empty() {
            project_path_under_home_from_user_relative(os_user, user_rel)
                .map_err(Status::invalid_argument)?
        } else {
            let base = repos_base_for_user(os_user, self.config.repos_base_path_or_default())
                .ok_or_else(|| Status::internal("could not resolve repos base path"))?;
            base.join(name)
        };
        let spawn_client = self.spawn_client.clone();
        let os_user_owned = os_user.to_string();
        let git_url_owned = git_url.to_string();
        let dest_path = destination.clone();
        let timeout = self.config.spawn_worker_request_timeout();

        match tddy_spawn::supervisor_client::spawn_backend_choice(&self.config) {
            tddy_spawn::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                service_util::await_supervised_with_timeout(
                    timeout,
                    "create_project: clone via tddy-supervisor",
                    tddy_spawn::supervisor_spawn::clone_repo_via_supervisor(
                        &socket_path,
                        &os_user_owned,
                        &git_url_owned,
                        &dest_path,
                    ),
                )
                .await?
            }
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                service_util::spawn_blocking_with_timeout(
                    timeout,
                    "create_project: clone_repo",
                    move || {
                        if let Some(ref client) = spawn_client {
                            client.clone_repo(spawn_worker::CloneRequest {
                                os_user: os_user_owned,
                                git_url: git_url_owned,
                                destination: dest_path.display().to_string(),
                            })
                        } else {
                            spawner::clone_as_user(&os_user_owned, &git_url_owned, &dest_path)
                        }
                    },
                )
                .await?
            }
        }

        let main_repo_path = destination
            .canonicalize()
            .unwrap_or(destination)
            .display()
            .to_string();

        let project = ProjectData {
            project_id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            git_url: git_url.to_string(),
            main_repo_path,
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: std::collections::HashMap::new(),
        };
        let repo_root = PathBuf::from(&project.main_repo_path);
        let default_remote = hooks_and_urls::resolve_default_remote_or_empty(
            &projects_dir,
            &project.project_id,
            &repo_root,
        );
        let entry = hooks_and_urls::project_entry_from(
            &project,
            local_instance_id_for_config(&self.config),
            default_remote,
        );
        project_storage::add_project(&projects_dir, project)
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateProjectResponse {
            project: Some(entry),
        }))
    }

    async fn add_project_to_host(
        &self,
        request: Request<AddProjectToHostRequest>,
    ) -> Result<Response<AddProjectToHostResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let project_id = req.project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }
        let name = req.name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("project name is required"));
        }
        if name.contains('/') || name.contains("..") {
            return Err(Status::invalid_argument("invalid project name"));
        }
        let git_url = req.git_url.trim();
        if git_url.is_empty() {
            return Err(Status::invalid_argument("git_url is required"));
        }

        // Route to the requested host: local (empty / matching id) or forward to a peer daemon.
        let requested_daemon = req.daemon_instance_id.trim();
        let local_id = local_instance_id_for_config(&self.config);
        let eligible_ids: Vec<String> = self
            .eligible_daemon_source
            .list_eligible_daemons()
            .iter()
            .map(|e| e.instance_id.0.clone())
            .collect();
        let route = crate::livekit_peer_discovery::classify_peer_route(
            &local_id,
            requested_daemon,
            &eligible_ids,
        )
        .map_err(|msg| {
            log::info!("AddProjectToHost: rejected daemon routing: {}", msg);
            Status::failed_precondition(msg)
        })?;

        if let crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id } = route {
            log::info!(
                "AddProjectToHost: forwarding RPC to remote daemon_instance_id={}",
                peer_instance_id
            );
            let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                Status::failed_precondition(
                    "cannot forward AddProjectToHost: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)",
                )
            })?;
            let inner = tddy_daemon_livekit::livekit_peer_discovery::forward_add_project_to_host_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(inner));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;

        // Idempotent: if this host already registers the project_id, return it without re-cloning.
        if let Some(existing) = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
        {
            log::info!(
                "AddProjectToHost: project_id={} already present on this host, returning existing row",
                project_id
            );
            let repo_root = PathBuf::from(&existing.main_repo_path);
            let default_remote = hooks_and_urls::resolve_default_remote_or_empty(
                &projects_dir,
                &existing.project_id,
                &repo_root,
            );
            return Ok(Response::new(AddProjectToHostResponse {
                project: Some(hooks_and_urls::project_entry_from(
                    &existing,
                    local_id,
                    default_remote,
                )),
            }));
        }

        let user_rel = req.user_relative_path.trim();
        let destination = if !user_rel.is_empty() {
            project_path_under_home_from_user_relative(os_user, user_rel)
                .map_err(Status::invalid_argument)?
        } else {
            let base = repos_base_for_user(os_user, self.config.repos_base_path_or_default())
                .ok_or_else(|| Status::internal("could not resolve repos base path"))?;
            base.join(name)
        };
        let spawn_client = self.spawn_client.clone();
        let os_user_owned = os_user.to_string();
        let git_url_owned = git_url.to_string();
        let dest_path = destination.clone();
        let timeout = self.config.spawn_worker_request_timeout();

        match tddy_spawn::supervisor_client::spawn_backend_choice(&self.config) {
            tddy_spawn::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                service_util::await_supervised_with_timeout(
                    timeout,
                    "add_project_to_host: clone via tddy-supervisor",
                    tddy_spawn::supervisor_spawn::clone_repo_via_supervisor(
                        &socket_path,
                        &os_user_owned,
                        &git_url_owned,
                        &dest_path,
                    ),
                )
                .await?
            }
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                service_util::spawn_blocking_with_timeout(
                    timeout,
                    "add_project_to_host: clone_repo",
                    move || {
                        if let Some(ref client) = spawn_client {
                            client.clone_repo(spawn_worker::CloneRequest {
                                os_user: os_user_owned,
                                git_url: git_url_owned,
                                destination: dest_path.display().to_string(),
                            })
                        } else {
                            spawner::clone_as_user(&os_user_owned, &git_url_owned, &dest_path)
                        }
                    },
                )
                .await?
            }
        }

        let main_repo_path = destination
            .canonicalize()
            .unwrap_or(destination)
            .display()
            .to_string();

        let main_branch_ref = {
            let r = req.main_branch_ref.trim();
            (!r.is_empty()).then(|| r.to_string())
        };
        let project = ProjectData {
            project_id: project_id.to_string(),
            name: name.to_string(),
            git_url: git_url.to_string(),
            main_repo_path,
            main_branch_ref,
            remote_name: None,
            host_repo_paths: std::collections::HashMap::new(),
        };
        let (stored, _created) = project_storage::add_or_get_project(&projects_dir, project)
            .map_err(|e| Status::internal(e.to_string()))?;

        let repo_root = PathBuf::from(&stored.main_repo_path);
        let default_remote = hooks_and_urls::resolve_default_remote_or_empty(
            &projects_dir,
            &stored.project_id,
            &repo_root,
        );
        Ok(Response::new(AddProjectToHostResponse {
            project: Some(hooks_and_urls::project_entry_from(
                &stored,
                local_id,
                default_remote,
            )),
        }))
    }

    async fn set_project_default_branch(
        &self,
        request: Request<SetProjectDefaultBranchRequest>,
    ) -> Result<Response<SetProjectDefaultBranchResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let project_id = req.project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }

        // Route to the requested host: local (empty / matching id) or forward to a peer daemon.
        let requested_daemon = req.daemon_instance_id.trim();
        let local_id = local_instance_id_for_config(&self.config);
        let eligible_ids: Vec<String> = self
            .eligible_daemon_source
            .list_eligible_daemons()
            .iter()
            .map(|e| e.instance_id.0.clone())
            .collect();
        let route = crate::livekit_peer_discovery::classify_peer_route(
            &local_id,
            requested_daemon,
            &eligible_ids,
        )
        .map_err(|msg| {
            log::info!("SetProjectDefaultBranch: rejected daemon routing: {}", msg);
            Status::failed_precondition(msg)
        })?;

        if let crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id } = route {
            log::info!(
                "SetProjectDefaultBranch: forwarding RPC to remote daemon_instance_id={}",
                peer_instance_id
            );
            let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                Status::failed_precondition(
                    "cannot forward SetProjectDefaultBranch: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)",
                )
            })?;
            let inner =
                tddy_daemon_livekit::livekit_peer_discovery::forward_set_project_default_branch_via_livekit(
                    slot,
                    &peer_instance_id,
                    &req,
                )
                .await?;
            return Ok(Response::new(inner));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;

        // Validate the ref shape and project existence up front so the client gets precise codes
        // (invalid_argument / not_found) before any registry mutation.
        tddy_core::validate_chain_pr_integration_base_ref(req.main_branch_ref.trim())
            .map_err(|e| Status::invalid_argument(format!("invalid main_branch_ref: {e}")))?;
        if project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .is_none()
        {
            return Err(Status::not_found("project not found"));
        }

        project_storage::set_project_default_branch(
            &projects_dir,
            project_id,
            req.main_branch_ref.trim(),
        )
        .map_err(|e| Status::internal(e.to_string()))?;

        let stored = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::internal("project vanished after write"))?;
        log::info!(
            "SetProjectDefaultBranch: project_id={} main_branch_ref={}",
            project_id,
            stored.main_branch_ref.as_deref().unwrap_or_default()
        );
        let repo_root = PathBuf::from(&stored.main_repo_path);
        let default_remote = hooks_and_urls::resolve_default_remote_or_empty(
            &projects_dir,
            &stored.project_id,
            &repo_root,
        );
        Ok(Response::new(SetProjectDefaultBranchResponse {
            project: Some(hooks_and_urls::project_entry_from(
                &stored,
                local_id,
                default_remote,
            )),
        }))
    }

    async fn start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.start_session_core(request.into_inner(), &AttachmentProgressSink::discarding())
            .await
    }

    async fn connect_session(
        &self,
        request: Request<ConnectSessionRequest>,
    ) -> Result<Response<ConnectSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        let session_type = metadata.session_type.as_deref().unwrap_or_default();

        // Connecting to a session is what opens its room. Session creation does not touch LiveKit
        // at all, so this is the moment the daemon has to be in the room a participant is about to
        // look for — including a `tddy-session-sync` mirror, which joins directly and waits for
        // `daemon-{instance_id}` to already be there.
        //
        // Only for a session this daemon facilitates over a checkout it holds. A session whose
        // agent runs elsewhere has its room over there, and opening one here would put it on a
        // daemon that serves nobody.
        if tddy_daemon_livekit::session_room::session_type_is_facilitated_here(session_type) {
            match metadata.repo_path.as_deref() {
                Some(worktree_root) => {
                    self.ensure_session_room(
                        &req.session_id,
                        &session_dir,
                        Path::new(worktree_root),
                    )
                    .await?;
                }
                None => log::debug!(
                    "ConnectSession: session {} records no checkout on this daemon, so its room is \
                     not this daemon's to open",
                    req.session_id
                ),
            }
        }

        // The *terminal* room, which is a different room with different participants. A claude-cli,
        // cursor-cli or workspace session has none: its PTY is served over gRPC and bridged into
        // the lobby, never into a room of its own, so these three answer with empty coordinates —
        // as they did before the session room above existed.
        if matches!(session_type, "claude-cli" | "cursor-cli" | "workspace") {
            return Ok(Response::new(ConnectSessionResponse {
                livekit_room: String::new(),
                livekit_url: String::new(),
                livekit_server_identity: String::new(),
            }));
        }

        let livekit_url = self
            .config
            .livekit
            .as_ref()
            .and_then(|l| l.public_url.clone())
            .or_else(|| self.config.livekit.as_ref().and_then(|l| l.url.clone()))
            .ok_or_else(|| Status::internal("LiveKit URL not configured"))?;
        let livekit_room = metadata
            .livekit_room
            .ok_or_else(|| Status::failed_precondition("session has no LiveKit room"))?;
        let instance = spawner::livekit_spawn_daemon_instance_id(&self.config);
        let livekit_server_identity =
            spawner::livekit_server_identity_for_session(instance.as_deref(), &req.session_id);
        log::debug!(
            "ConnectSession: livekit_server_identity={} session_id={}",
            livekit_server_identity,
            req.session_id
        );
        Ok(Response::new(ConnectSessionResponse {
            livekit_room,
            livekit_url,
            livekit_server_identity,
        }))
    }

    async fn resume_session(
        &self,
        request: Request<ResumeSessionRequest>,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        // --- claude-cli branch: resume without LiveKit ---
        if metadata.session_type.as_deref() == Some("claude-cli") {
            return self
                .resume_claude_cli_session(
                    os_user,
                    &req.session_id,
                    session_dir,
                    metadata,
                    &req.session_token,
                )
                .await;
        }

        // --- cursor-cli branch: resume without LiveKit ---
        if metadata.session_type.as_deref() == Some("cursor-cli") {
            return crate::cursor_cli_spawn::resume_cursor_cli_session(
                &self.claude_cli_manager,
                &self.config,
                &req.session_id,
                &session_dir,
                metadata,
            )
            .await;
        }

        // --- workspace branch: re-provision jail when sandboxed, no LiveKit ---
        if metadata.session_type.as_deref() == Some("workspace") {
            if metadata.sandbox == Some(true)
                && self
                    .workspace_sandboxes
                    .get(&req.session_id)
                    .await
                    .is_none()
            {
                self.provision_workspace_tool_sandbox(&sessions_base, &req.session_id)
                    .await?;
            }
            return Ok(Response::new(ResumeSessionResponse {
                session_id: req.session_id,
                livekit_room: String::new(),
                livekit_url: String::new(),
                livekit_server_identity: String::new(),
            }));
        }

        let repo_path = metadata
            .repo_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| session_dir.clone());
        let repo_path = if repo_path.exists() {
            repo_path
        } else {
            session_dir.clone()
        };
        let tool_path = metadata
            .tool
            .clone()
            .ok_or_else(|| Status::failed_precondition("session has no recorded tool path"))?;
        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| Status::failed_precondition("LiveKit not configured"))?;
        let spawn_client = self.spawn_client.clone();
        let spawn_mouse = self.config.spawn_mouse;
        let os_user = os_user.to_string();
        // The spawn closure below takes ownership; the presenter observer started afterwards needs
        // the same user to resolve the session's label from its sessions directory.
        let observer_os_user = os_user.clone();
        let session_id = req.session_id.clone();
        let livekit = livekit.clone();
        let project_id_resume = metadata.project_id.clone();
        let (resume_agent, resume_recipe) = service_util::resume_agent_and_recipe(&metadata);
        // A resumed session's agent is resolved the same way a starting one's is, so an assistant
        // this daemon defined still reaches the child as a def it can build a backend from.
        let resume_agent_def: Option<String> = match resume_agent.as_deref() {
            Some(name) => self
                .agent_def_for_spawn(name, &github_user)
                .await?
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| Status::internal(format!("failed to serialize agent def: {e}")))?,
            None => None,
        };
        let tddy_data_dir_for_spawn = self.tddy_data_dir.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let daemon_log = self.config.log.clone();
        let startup_watch = spawner::StartupWatch::from_config(&self.config);
        let coder_config_path = self.config.coder_config_path.clone();
        let result = match tddy_spawn::supervisor_client::spawn_backend_choice(&self.config) {
            tddy_spawn::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                let coder_log_yaml = spawner::coder_log_config_yaml(coder_config_path.as_deref());
                let spawn_req = spawn_worker::build_spawn_request(
                    &os_user,
                    &tool_path,
                    &tddy_data_dir_for_spawn,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: Some(session_id.as_str()),
                        new_session_id: None,
                        project_id: Some(project_id_resume.as_str()).filter(|id| !id.is_empty()),
                        agent: resume_agent.as_deref(),
                        agent_def_json: resume_agent_def.as_deref(),
                        mouse: spawn_mouse,
                        recipe: resume_recipe.as_deref(),
                        stack_parent: None,
                        stack_node_id: None,
                        // Seeding a stack is a creation-time act; a resumed orchestrator already has
                        // whatever stack it was created with.
                        stack_seed_base_session: None,
                        model: None,
                        // TODO(stdio-relay): wire the resume path's reverse channel too.
                        host_session_socket: None,
                    },
                    daemon_log.as_ref(),
                    coder_log_yaml,
                    startup_watch,
                );
                service_util::await_supervised_with_timeout(
                    timeout,
                    "ResumeSession: spawn via tddy-supervisor",
                    tddy_spawn::supervisor_spawn::spawn_session_via_supervisor(
                        &socket_path,
                        &spawn_req,
                    ),
                )
                .await?
            }
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                service_util::spawn_blocking_with_timeout(
                    timeout,
                    "ResumeSession: spawn",
                    move || {
                        let pid = if project_id_resume.is_empty() {
                            None
                        } else {
                            Some(project_id_resume.as_str())
                        };
                        let coder_log_yaml =
                            spawner::coder_log_config_yaml(coder_config_path.as_deref());
                        if let Some(ref client) = spawn_client {
                            let spawn_req = spawn_worker::build_spawn_request(
                                &os_user,
                                &tool_path,
                                &tddy_data_dir_for_spawn,
                                &repo_path,
                                &livekit,
                                SpawnOptions {
                                    resume_session_id: Some(session_id.as_str()),
                                    new_session_id: None,
                                    project_id: pid,
                                    agent: resume_agent.as_deref(),
                                    agent_def_json: resume_agent_def.as_deref(),
                                    mouse: spawn_mouse,
                                    recipe: resume_recipe.as_deref(),
                                    stack_parent: None,
                                    stack_node_id: None,
                                    // Seeding a stack is a creation-time act; a resumed orchestrator
                                    // already has whatever stack it was created with.
                                    stack_seed_base_session: None,
                                    model: None,
                                    // TODO(stdio-relay): wire the resume path's reverse channel too.
                                    host_session_socket: None,
                                },
                                daemon_log.as_ref(),
                                coder_log_yaml,
                                startup_watch,
                            );
                            client.spawn(spawn_req)
                        } else {
                            let (child_log_level, child_log_format) =
                                spawner::child_log_yaml_tuning(daemon_log.as_ref());
                            spawner::spawn_as_user(
                                &os_user,
                                &tool_path,
                                &tddy_data_dir_for_spawn,
                                &repo_path,
                                &livekit,
                                SpawnOptions {
                                    resume_session_id: Some(session_id.as_str()),
                                    new_session_id: None,
                                    project_id: pid,
                                    agent: resume_agent.as_deref(),
                                    agent_def_json: resume_agent_def.as_deref(),
                                    mouse: spawn_mouse,
                                    recipe: resume_recipe.as_deref(),
                                    stack_parent: None,
                                    stack_node_id: None,
                                    // Seeding a stack is a creation-time act; a resumed orchestrator
                                    // already has whatever stack it was created with.
                                    stack_seed_base_session: None,
                                    model: None,
                                    // TODO(stdio-relay): wire the resume path's reverse channel too.
                                    host_session_socket: None,
                                },
                                child_log_level.as_str(),
                                child_log_format.as_str(),
                                coder_log_yaml.as_deref(),
                                startup_watch,
                            )
                        }
                    },
                )
                .await?
            }
        };
        self.maybe_spawn_presenter_observer(
            &observer_os_user,
            &result.session_id,
            result.grpc_port,
        );
        Ok(Response::new(ResumeSessionResponse {
            session_id: result.session_id,
            livekit_room: result.livekit_room,
            livekit_url: result.livekit_url,
            livekit_server_identity: result.livekit_server_identity,
        }))
    }

    async fn signal_session(
        &self,
        request: Request<SignalSessionRequest>,
    ) -> Result<Response<SignalSessionResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "SignalSession: session_id={}, signal={}",
            req.session_id,
            req.signal
        );

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        let pid = metadata
            .pid
            .ok_or_else(|| Status::failed_precondition("session has no PID"))?;

        log::debug!(
            "SignalSession: resolved pid={} for session={}",
            pid,
            req.session_id
        );

        #[cfg(unix)]
        {
            use tddy_service::proto::connection::Signal;

            let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
            if !alive {
                log::debug!("SignalSession: pid={} is not alive", pid);
                return Err(Status::failed_precondition("process is not alive"));
            }

            let os_signal = match Signal::try_from(req.signal) {
                Ok(Signal::Sigint) => libc::SIGINT,
                Ok(Signal::Sigterm) => libc::SIGTERM,
                Ok(Signal::Sigkill) => libc::SIGKILL,
                Err(_) => return Err(Status::invalid_argument("invalid signal value")),
            };

            log::info!(
                "SignalSession: sending signal {} to pid={} session={}",
                os_signal,
                pid,
                req.session_id
            );

            let ret = unsafe { libc::kill(pid as i32, os_signal) };
            if ret != 0 {
                let err = std::io::Error::last_os_error();
                log::error!(
                    "SignalSession: kill({}, {}) failed: {}",
                    pid,
                    os_signal,
                    err
                );
                return Err(Status::internal(format!("failed to send signal: {}", err)));
            }

            Ok(Response::new(SignalSessionResponse {
                ok: true,
                message: format!("signal {} sent to pid {}", os_signal, pid),
            }))
        }

        #[cfg(not(unix))]
        {
            let _ = pid;
            Err(Status::unimplemented(
                "signal delivery is only supported on Unix",
            ))
        }
    }

    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }
        log::debug!("DeleteSession: requested session_id={}", session_id);
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        log::debug!(
            "DeleteSession: resolved sessions_base={:?} for os_user={}",
            sessions_base,
            os_user
        );
        let projects_dir_opt = projects_path_for_user(os_user, Some(&self.tddy_data_dir));
        // A split session's worktree lives on another daemon, which must lose it first: deleting
        // this side alone would leave a checkout on a host with no session left to reclaim it. A
        // failure to reach that daemon fails the delete rather than silently dropping its half.
        self.delete_paired_codebase_session(&sessions_base, session_id, &req.session_token)
            .await?;
        // Every clone this session's roster created, on every host that built one — including hosts
        // the operator never looked at. Refused rather than continued if one cannot be reached, for
        // the same reason the paired workspace above is: a delete that succeeded here while a
        // checkout survived elsewhere is exactly the silent leak this is for.
        self.tear_down_every_agent_clone(session_id, &req.session_token)
            .await?;
        // Every admission this session minted is void with the session: a mirror that re-admits
        // after the delete must be refused, and `revoke_all_for_session` is the bulk revocation
        // that does it. (Per-daemon revocation on the last detach is in `tear_down_agent_clone`;
        // this is the session-wide sweep that catches admissions whose clones were already gone.)
        let revoked = self.session_admissions.revoke_all_for_session(session_id);
        if revoked > 0 {
            log::info!("revoked {revoked} admission(s) for session {session_id} on session delete");
        }
        // The other direction: this daemon may be *holding* a clone, whose workspace session is the
        // one being deleted. Forgetting it before the directory goes is what stops a tool call
        // arriving a moment later from being served out of a checkout that no longer exists.
        self.hosted_agent_clones.forget_checkout(session_id);
        // The conversation this daemon was tailing goes with the session: its consumer task exits on
        // the next record rather than holding a subscription for the daemon's life, and a session id
        // reused later starts from nothing observed instead of the deleted session's last call.
        self.session_agent_inference.forget(session_id);
        if let Some(sandbox) = self.sandbox_manager.get(session_id).await {
            sandbox.stop();
        }
        let _ = self.sandbox_manager.remove(session_id).await;
        // The workspace jail goes the same way, and *before* the directory does: the jail is a live
        // process holding the worktree open, so a jail left registered would go on running against
        // a checkout that no longer exists — for the rest of this daemon's life, since the registry
        // is the only thing holding it. Dropping the last handle stops it.
        if let Some(jail) = self.workspace_sandboxes.remove(session_id).await {
            jail.stop();
        }
        session_deletion::close_session_room(&self.session_rooms, session_id);
        session_deletion::delete_session_directory(
            &sessions_base,
            session_id,
            projects_dir_opt.as_deref(),
        )?;
        log::info!("DeleteSession: successfully removed session {}", session_id);
        Ok(Response::new(DeleteSessionResponse { ok: true }))
    }

    type StreamAgentActivityDeltaStream = MpscResultStream<AgentActivityDeltaChunk>;

    /// Families M and N moved to `activity.ActivityService` in `#unbundle` node 7. This coordinate
    /// keeps answering all eight by delegating to that surface — the routed one, so a request
    /// naming another daemon is forwarded (or refused) there exactly as it was here.
    ///
    /// TODO(session-agent-services): the eight delegations go when this coordinate stops declaring
    /// these rpcs, which is the next milestone. Nothing new should be added to them.
    async fn stream_agent_activity_delta(
        &self,
        request: Request<AgentActivityDeltaRequest>,
    ) -> Result<Response<Self::StreamAgentActivityDeltaStream>, Status> {
        let req = request.into_inner();
        let frames = self
            .activity_surface()
            .stream_agent_activity_delta(Request::new(
                tddy_service::proto::activity::AgentActivityDeltaRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    call_id: req.call_id,
                    scope: req.scope,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(relayed_onto_this_coordinate(
            frames,
            |chunk: tddy_service::proto::activity::AgentActivityDeltaChunk| {
                AgentActivityDeltaChunk {
                    patch: chunk.patch,
                    seq: chunk.seq,
                    prev_seq: chunk.prev_seq,
                    base_commit: chunk.base_commit,
                    total_byte_size: chunk.total_byte_size,
                    scoped_paths: chunk.scoped_paths,
                }
            },
        )))
    }

    async fn list_project_branches(
        &self,
        request: Request<ListProjectBranchesRequest>,
    ) -> Result<Response<ListProjectBranchesResponse>, Status> {
        const BRANCH_LIST_LIMIT: usize = 50;

        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let project_id = req.project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        if !repo_root.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        let timeout = self.config.spawn_worker_request_timeout();
        let remote = project_storage::effective_remote_name_for_project(
            &projects_dir,
            project_id,
            &repo_root,
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        let remote_for_closure = remote.clone();
        let branches = service_util::spawn_blocking_with_timeout(
            timeout,
            "ListProjectBranches: git remote refs",
            move || {
                tddy_core::list_recent_remote_branches(
                    &repo_root,
                    &remote_for_closure,
                    BRANCH_LIST_LIMIT,
                )
                .map_err(|e| anyhow::anyhow!("list_recent_remote_branches failed: {}", e))
            },
        )
        .await?;

        log::debug!(
            target: "tddy_daemon::connection_service",
            "list_project_branches: project_id={} returned {} branches",
            project_id,
            branches.len()
        );

        Ok(Response::new(ListProjectBranchesResponse {
            branches,
            default_remote: remote,
        }))
    }

    async fn execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<ExecuteToolResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay (which has no local sessions) can forward.
        if let Some(answered) = self
            .rpc_served_by_peer("ExecuteTool", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        // Auth before *any* worktree is chosen, because the hosted-clone branch below chooses one
        // that is not this daemon's and proxies its mutations under the clone's own credential.
        self.authorize_exec_tool_caller(&req)?;

        // A session this daemon holds an *agent clone* for lives on another daemon, so the ordinary
        // "resolve the worktree from my own sessions base" would find nothing. Checked before that
        // resolution rather than after it, so the read/write split is what answers rather than a
        // not-found for a session that legitimately is not here.
        if let Some(clone) = self.hosted_clone_for(&req.session_id) {
            reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
            return Ok(Response::new(
                self.run_hosted_clone_tool(&req, &clone).await,
            ));
        }

        let (sessions_base, worktree_root) = self.resolve_exec_tool_worktree(&req)?;
        reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
        let response = self
            .run_exec_tool_locally(&req, &sessions_base, &worktree_root)
            .await;
        Ok(Response::new(response))
    }

    /// Associated output stream type for [`stream_execute_tool`].
    type StreamExecuteToolStream = MpscResultStream<ExecuteToolChunk>;

    /// Server-streaming sibling of [`Self::execute_tool`], carrying the same result in bounded
    /// frames.
    ///
    /// The unary call returns `result_json` as one string; over LiveKit anything past
    /// `MAX_CHUNK_FRAME_BYTES` is chunk-framed, and one lost chunk frame wedges the call with no
    /// error at all (`docs/ft/coder/rpc-multi-transport.md`). A `Read` of a large file crosses that
    /// on day one of a split session, so the split path streams instead — routing, auth and worktree
    /// resolution are shared with the unary handler so the two cannot drift.
    async fn stream_execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<Self::StreamExecuteToolStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_addressed_daemon_route("StreamExecuteTool", &req.daemon_instance_id)?
        {
            let slot = self.common_room_slot("StreamExecuteTool")?;
            // A forwarded stream that stalls terminates as an *error*, so a truncated tool result
            // can never reach the caller looking complete.
            let rx = tddy_daemon_livekit::livekit_peer_discovery::forward_stream_execute_tool_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(MpscResultStream { rx }));
        }

        // See the unary handler: auth first, because the hosted-clone branch resolves no worktree of
        // this daemon's and would otherwise be reachable with no credential at all.
        self.authorize_exec_tool_caller(&req)?;

        // A session this daemon holds an agent clone for is served by the read/write split, from a
        // checkout that is not in this daemon's own sessions base.
        let response = match self.hosted_clone_for(&req.session_id) {
            Some(clone) => {
                reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
                self.run_hosted_clone_tool(&req, &clone).await
            }
            None => {
                let (sessions_base, worktree_root) = self.resolve_exec_tool_worktree(&req)?;
                reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
                self.run_exec_tool_locally(&req, &sessions_base, &worktree_root)
                    .await
            }
        };

        // The result is already complete in memory, so every frame can be queued now: the stream
        // exists to bound each frame's size, not to interleave with the tool's execution.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<ExecuteToolChunk, Status>>();
        for frame in exec_tool_result_frames(response) {
            if tx.send(Ok(frame)).is_err() {
                break;
            }
        }
        Ok(Response::new(MpscResultStream { rx }))
    }

    async fn list_exec_tools(
        &self,
        request: Request<ListExecToolsRequest>,
    ) -> Result<Response<ListExecToolsResponse>, Status> {
        let req = request.into_inner();

        // Route BEFORE auth so a relay (which has no local user table) can forward.
        let requested_daemon = req.daemon_instance_id.trim();
        if !requested_daemon.is_empty() {
            let local_id = local_instance_id_for_config(&self.config);
            let eligible_rows = self.eligible_daemon_source.list_eligible_daemons();
            let eligible_ids: Vec<String> = eligible_rows
                .iter()
                .map(|e| e.instance_id.0.clone())
                .collect();
            match crate::livekit_peer_discovery::classify_peer_route(
                &local_id,
                requested_daemon,
                &eligible_ids,
            ) {
                Err(msg) => {
                    log::info!("ListExecTools: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "ListExecTools: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward ListExecTools: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "connection.ConnectionService",
                        "ListExecTools",
                        body,
                    )
                    .await?;
                    let inner = ListExecToolsResponse::decode(out.as_slice()).map_err(|e| {
                        Status::internal(format!("decode ListExecToolsResponse: {e}"))
                    })?;
                    return Ok(Response::new(inner));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Minimal auth — verify caller is a known user.
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        Ok(Response::new(ListExecToolsResponse {
            tools: tool_engine::tool_catalog()
                .into_iter()
                .map(|t| tddy_service::proto::connection::ToolDef {
                    name: t.name,
                    description: t.description,
                    input_schema_json: t.input_schema_json,
                })
                .collect(),
        }))
    }

    async fn list_session_tool_calls(
        &self,
        request: Request<ListSessionToolCallsRequest>,
    ) -> Result<Response<ListSessionToolCallsResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay can forward.
        let requested_daemon = req.daemon_instance_id.trim();
        if !requested_daemon.is_empty() {
            let local_id = local_instance_id_for_config(&self.config);
            let eligible_rows = self.eligible_daemon_source.list_eligible_daemons();
            let eligible_ids: Vec<String> = eligible_rows
                .iter()
                .map(|e| e.instance_id.0.clone())
                .collect();
            match crate::livekit_peer_discovery::classify_peer_route(
                &local_id,
                requested_daemon,
                &eligible_ids,
            ) {
                Err(msg) => {
                    log::info!("ListSessionToolCalls: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "ListSessionToolCalls: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward ListSessionToolCalls: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "connection.ConnectionService",
                        "ListSessionToolCalls",
                        body,
                    )
                    .await?;
                    let inner =
                        ListSessionToolCallsResponse::decode(out.as_slice()).map_err(|e| {
                            Status::internal(format!("decode ListSessionToolCallsResponse: {e}"))
                        })?;
                    return Ok(Response::new(inner));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Authenticate caller.
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        // Validate session ID segment to prevent path traversal.
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        // Resolve the sessions base path.
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;

        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        let records = crate::tool_call_log::read_tool_calls(&session_dir).unwrap_or_default();

        let tool_calls: Vec<ProtoToolCallInfo> = records
            .into_iter()
            .map(|r| ProtoToolCallInfo {
                task_id: r.task_id,
                tool_name: r.tool_name,
                args_json: r.args_json,
                result_json: r.result_json,
                is_error: r.is_error,
                error_message: r.error_message,
                job_running: r.job_running,
                created_unix_ms: r.created_unix_ms,
            })
            .collect();

        Ok(Response::new(ListSessionToolCallsResponse { tool_calls }))
    }

    async fn report_session_status(
        &self,
        request: Request<ReportSessionStatusRequest>,
    ) -> Result<Response<ReportSessionStatusResponse>, Status> {
        let req = request.into_inner();
        let answer = self
            .activity_surface()
            .report_session_status(Request::new(
                tddy_service::proto::activity::ReportSessionStatusRequest {
                    session_id: req.session_id,
                    hook_token: req.hook_token,
                    os_user: req.os_user,
                    status: req.status,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(ReportSessionStatusResponse { ok: answer.ok }))
    }

    async fn report_agent_activity(
        &self,
        request: Request<ReportAgentActivityRequest>,
    ) -> Result<Response<ReportAgentActivityResponse>, Status> {
        let req = request.into_inner();
        let answer = self
            .activity_surface()
            .report_agent_activity(Request::new(
                tddy_service::proto::activity::ReportAgentActivityRequest {
                    session_id: req.session_id,
                    hook_token: req.hook_token,
                    os_user: req.os_user,
                    event: req.event,
                    tool_name: req.tool_name,
                    input_json: req.input_json,
                    result_json: req.result_json,
                    is_error: req.is_error,
                    error_message: req.error_message,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(ReportAgentActivityResponse { ok: answer.ok }))
    }

    async fn start_demo_vm(
        &self,
        request: Request<StartDemoVmRequest>,
    ) -> Result<Response<StartDemoVmResponse>, Status> {
        let req = request.into_inner();
        self.record_rpc_activity();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        // Read demo-plan.md from the session directory.
        let demo_plan = tddy_workflow_recipes::writer::read_demo_plan_file(&session_dir)
            .map_err(|e| Status::not_found(format!("demo-plan.md not found: {e}")))?;

        let qcow2_path = demo_plan
            .build_target
            .ok_or_else(|| Status::failed_precondition("demo-plan.md has no build_target"))?;
        // ssh_host_port defaults to 2222; the first hostfwd entry is the app port, not SSH.
        let ssh_host_port: u16 = 2222;
        let config = tddy_vm::VmConfig {
            qcow2_path,
            extra_hostfwd: demo_plan
                .hostfwd
                .iter()
                .map(|p| tddy_vm::PortForward {
                    host_port: p.host_port,
                    guest_port: p.guest_port,
                })
                .collect(),
            ssh_host_port,
            // Pinned to what the launcher did before any of this was configurable, because
            // nothing here knows the demo image's architecture — `demo-plan.md` names a
            // build target, and `tddy-build-qemu` produces x86_64 images. Deriving these
            // from the *host* would run an aarch64 emulator against an x86_64 image on an
            // Apple Silicon machine, and `virt` has no BIOS for it to fall back to.
            // TCG likewise matches the previous behaviour and avoids making the demo path
            // newly dependent on the daemon user's access to /dev/kvm.
            arch: tddy_vm::VmArch::X86_64,
            accel: tddy_vm::VmAccel::Tcg,
            // The resources the launcher hard-coded before they were configurable.
            memory: "512M".to_string(),
            cpus: 1,
            // The demo images boot through their own BIOS; they carry no cloud-init seed
            // and share nothing from the host.
            firmware: None,
            login: tddy_vm::VmLogin {
                username: "root".to_string(),
                private_key_path: None,
            },
            seed_iso: None,
            nine_p_shares: vec![],
        };

        // Reject if already booting/running for this session.
        {
            let state = self.demo_vm_state.lock().await;
            if let Some(h) = state.get(&req.session_id) {
                let (state_enum, msg) = match h {
                    activity_hub::DemoVmHandle::Booting => {
                        (DemoVmState::Booting, "already booting")
                    }
                    activity_hub::DemoVmHandle::Running { .. } => {
                        (DemoVmState::Running, "VM already running")
                    }
                    activity_hub::DemoVmHandle::Error(_) => {
                        // Allow retry after error.
                        return Ok(Response::new(StartDemoVmResponse {
                            state: DemoVmState::Booting as i32,
                            message: "retrying after previous error".to_string(),
                        }));
                    }
                };
                return Ok(Response::new(StartDemoVmResponse {
                    state: state_enum as i32,
                    message: msg.to_string(),
                }));
            }
        }

        // Mark as booting and spawn the boot task.
        {
            let mut state = self.demo_vm_state.lock().await;
            state.insert(req.session_id.clone(), activity_hub::DemoVmHandle::Booting);
        }

        // Build the share URL from the first app hostfwd entry (not the SSH port itself).
        let share_url = config
            .extra_hostfwd
            .first()
            .map(|p| format!("http://localhost:{}", p.host_port))
            .unwrap_or_default();

        let state_ref = Arc::clone(&self.demo_vm_state);
        let session_id = req.session_id.clone();
        tokio::spawn(async move {
            use tddy_vm::Vm as _;
            let vm_impl = tddy_vm::QemuVm;
            match vm_impl.boot(&config).await {
                Ok(vm) => {
                    let mut state = state_ref.lock().await;
                    state.insert(
                        session_id,
                        activity_hub::DemoVmHandle::Running { vm, share_url },
                    );
                }
                Err(e) => {
                    let mut state = state_ref.lock().await;
                    state.insert(session_id, activity_hub::DemoVmHandle::Error(e.to_string()));
                }
            }
        });

        log::info!(
            "start_demo_vm: booting VM for session_id={}",
            req.session_id
        );
        Ok(Response::new(StartDemoVmResponse {
            state: DemoVmState::Booting as i32,
            message: "booting".to_string(),
        }))
    }

    async fn stop_demo_vm(
        &self,
        request: Request<StopDemoVmRequest>,
    ) -> Result<Response<StopDemoVmResponse>, Status> {
        let req = request.into_inner();
        self.record_rpc_activity();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let handle = {
            let mut state = self.demo_vm_state.lock().await;
            state.remove(&req.session_id)
        };

        match handle {
            Some(activity_hub::DemoVmHandle::Running { vm, .. }) => {
                use tddy_vm::Vm as _;
                let vm_impl = tddy_vm::QemuVm;
                match vm_impl.shutdown(vm).await {
                    Ok(()) => {
                        log::info!("stop_demo_vm: shutdown ok session_id={}", req.session_id);
                        Ok(Response::new(StopDemoVmResponse {
                            ok: true,
                            message: "shutdown".to_string(),
                        }))
                    }
                    Err(e) => Err(Status::internal(format!("shutdown failed: {e}"))),
                }
            }
            Some(activity_hub::DemoVmHandle::Booting) => Err(Status::failed_precondition(
                "VM is still booting; wait until Running before stopping",
            )),
            Some(activity_hub::DemoVmHandle::Error(msg)) => Ok(Response::new(StopDemoVmResponse {
                ok: true,
                message: format!("VM was in error state ({msg}); cleared"),
            })),
            None => Ok(Response::new(StopDemoVmResponse {
                ok: true,
                message: "no VM running for this session".to_string(),
            })),
        }
    }

    async fn get_demo_vm_status(
        &self,
        request: Request<GetDemoVmStatusRequest>,
    ) -> Result<Response<GetDemoVmStatusResponse>, Status> {
        let req = request.into_inner();
        self.record_rpc_activity();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let state = self.demo_vm_state.lock().await;
        let resp = match state.get(&req.session_id) {
            None => GetDemoVmStatusResponse {
                state: DemoVmState::Stopped as i32,
                ssh_host_port: 0,
                message: "no VM for this session".to_string(),
                share_url: String::new(),
            },
            Some(activity_hub::DemoVmHandle::Booting) => GetDemoVmStatusResponse {
                state: DemoVmState::Booting as i32,
                ssh_host_port: 0,
                message: "booting".to_string(),
                share_url: String::new(),
            },
            Some(activity_hub::DemoVmHandle::Running { vm, share_url }) => {
                GetDemoVmStatusResponse {
                    state: DemoVmState::Running as i32,
                    ssh_host_port: vm.ssh_host_port as u32,
                    message: "running".to_string(),
                    share_url: share_url.clone(),
                }
            }
            Some(activity_hub::DemoVmHandle::Error(msg)) => GetDemoVmStatusResponse {
                state: DemoVmState::Error as i32,
                ssh_host_port: 0,
                message: msg.clone(),
                share_url: String::new(),
            },
        };
        Ok(Response::new(resp))
    }

    // --- agent activity ---

    type StreamSessionActivityStream = MpscResultStream<ProtoAgentActivityRecord>;

    async fn stream_session_activity(
        &self,
        request: Request<StreamSessionActivityRequest>,
    ) -> Result<Response<Self::StreamSessionActivityStream>, Status> {
        let req = request.into_inner();
        let frames = self
            .activity_surface()
            .stream_session_activity(Request::new(
                tddy_service::proto::activity::StreamSessionActivityRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    mode: req.mode,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(relayed_onto_this_coordinate(
            frames,
            activity_record_at_the_old_coordinate,
        )))
    }

    type StreamSessionNotificationsStream = MpscResultStream<ProtoSessionNotificationEvent>;

    async fn stream_session_notifications(
        &self,
        request: Request<StreamSessionNotificationsRequest>,
    ) -> Result<Response<Self::StreamSessionNotificationsStream>, Status> {
        let req = request.into_inner();
        let frames = self
            .activity_surface()
            .stream_session_notifications(Request::new(
                tddy_service::proto::activity::StreamSessionNotificationsRequest {
                    session_token: req.session_token,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(relayed_onto_this_coordinate(
            frames,
            |event: tddy_service::proto::activity::SessionNotificationEvent| {
                ProtoSessionNotificationEvent {
                    session_id: event.session_id,
                    label: event.label,
                    kind: event.kind,
                    source: event.source,
                    text: event.text,
                    at_unix_ms: event.at_unix_ms,
                }
            },
        )))
    }

    type StreamAcpReplayStream = MpscResultStream<AcpReplayFrame>;

    async fn stream_acp_replay(
        &self,
        request: Request<StreamAcpReplayRequest>,
    ) -> Result<Response<Self::StreamAcpReplayStream>, Status> {
        let req = request.into_inner();
        let frames = self
            .activity_surface()
            .stream_acp_replay(Request::new(
                tddy_service::proto::activity::StreamAcpReplayRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    mode: req.mode,
                    page_size: req.page_size,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(relayed_onto_this_coordinate(
            frames,
            |frame: tddy_service::proto::activity::AcpReplayFrame| AcpReplayFrame {
                acp_agent_message: frame.acp_agent_message,
                activity_count: frame.activity_count,
                seq: frame.seq,
            },
        )))
    }

    async fn get_acp_tool_call_detail(
        &self,
        request: Request<GetAcpToolCallDetailRequest>,
    ) -> Result<Response<GetAcpToolCallDetailResponse>, Status> {
        let req = request.into_inner();
        let answer = self
            .activity_surface()
            .get_acp_tool_call_detail(Request::new(
                tddy_service::proto::activity::GetAcpToolCallDetailRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    tool_call_id: req.tool_call_id,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(GetAcpToolCallDetailResponse {
            raw_input: answer.raw_input,
            raw_output: answer.raw_output,
        }))
    }

    async fn get_acp_replay_page(
        &self,
        request: Request<GetAcpReplayPageRequest>,
    ) -> Result<Response<GetAcpReplayPageResponse>, Status> {
        let req = request.into_inner();
        let answer = self
            .activity_surface()
            .get_acp_replay_page(Request::new(
                tddy_service::proto::activity::GetAcpReplayPageRequest {
                    session_token: req.session_token,
                    session_id: req.session_id,
                    daemon_instance_id: req.daemon_instance_id,
                    before_seq: req.before_seq,
                    page_size: req.page_size,
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(GetAcpReplayPageResponse {
            frames: answer.frames,
            first_seq: answer.first_seq,
            at_oldest: answer.at_oldest,
        }))
    }

    // --- PR-Stack Chat Screen: manually adding a planned PR ---

    /// Append a manually-created planned PR to a "pr-stack" orchestrator session's stack,
    /// choosing its ancestors from the already-planned nodes. See
    /// `tddy_workflow_recipes::pr_stack::add_planned_pr_node`.
    async fn add_planned_pr(
        &self,
        request: Request<AddPlannedPrRequest>,
    ) -> Result<Response<AddPlannedPrResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "AddPlannedPr: session_id={} title={:?}",
            req.session_id.trim(),
            req.title
        );
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.title.trim().is_empty() {
            return Err(Status::invalid_argument("title is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        let branch_suggestion =
            (!req.branch_suggestion.trim().is_empty()).then(|| req.branch_suggestion.clone());
        let child_recipe = (!req.child_recipe.trim().is_empty()).then(|| req.child_recipe.clone());

        // The appended node, so the response can *name* what this call created. The caller must not
        // infer it by diffing the returned plan: the orchestrator agent appends nodes to the same
        // stack, so a plan can come back holding several nodes the caller has never seen.
        let added = tddy_workflow_recipes::pr_stack::add_planned_pr_node(
            &session_dir,
            tddy_workflow_recipes::pr_stack::AddPlannedPrInput {
                title: req.title.clone(),
                description: req.description.clone(),
                branch_suggestion,
                parents: req.parents.clone(),
                child_recipe,
            },
        )
        .map_err(Status::invalid_argument)?;

        // Re-read the just-updated changeset and reuse the same serializer as `ListSessions`
        // enrichment so the response's `stack_plan_json` is byte-for-byte the same wire shape
        // `PrStackScreen`'s `parseStackPlan` already knows how to read.
        let changeset =
            tddy_core::read_changeset(&session_dir).map_err(|e| Status::internal(e.to_string()))?;
        let stack_plan_json = session_list_enrichment::stack_plan_json_for_changeset(&changeset);

        log::info!(
            "AddPlannedPr: success session_id={} node_id={} title={:?}",
            req.session_id.trim(),
            added.node_id,
            req.title
        );
        Ok(Response::new(AddPlannedPrResponse {
            stack_plan_json,
            node_id: added.node_id,
        }))
    }

    async fn get_pr_status(
        &self,
        request: Request<GetPrStatusRequest>,
    ) -> Result<Response<GetPrStatusResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.branch.trim().is_empty() {
            return Err(Status::invalid_argument("branch is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        // An orchestrator never gets a worktree, so its `changeset.yaml` records no checkout and only
        // `.session.yaml` names the one it plans over. `pr_status_for_caller` resolves the GitHub
        // namespace from that checkout's remote and reports a lookup it cannot perform — including
        // one over an unknown checkout — as unavailable rather than failing this call.
        let repo_root = tddy_core::repo_root_for_session(&session_dir);

        let status = self
            .pr_status_for_caller(&github_user, repo_root.as_deref(), req.branch.trim())
            .await;
        Ok(Response::new(GetPrStatusResponse {
            status: Some(status),
        }))
    }

    async fn get_worktree_snapshot(
        &self,
        request: Request<GetWorktreeSnapshotRequest>,
    ) -> Result<Response<GetWorktreeSnapshotResponse>, Status> {
        let req = request.into_inner();

        // Routed exactly like ExecuteTool, and for the same reason: the caller names the daemon it
        // believes holds the checkout, and a session room on the agent's daemon polls a remote
        // checkout by addressing the codebase daemon. Sharing the classifier and the forward keeps
        // one answer to "which daemon owns this session's files".
        if let Some(answered) = self
            .rpc_served_by_peer("GetWorktreeSnapshot", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        // The measurement is assembled here, where the files are, and shells out to git — so it
        // runs on the blocking pool under the same budget a local poll uses. A caller that gave up
        // waiting is a caller whose next tick will ask again.
        let (sessions_base, worktree_root) =
            self.resolve_exec_tool_worktree(&ExecuteToolRequest {
                session_token: req.session_token.clone(),
                session_id: req.session_id.clone(),
                tool_name: "GetWorktreeSnapshot".to_string(),
                args_json: String::new(),
                daemon_instance_id: req.daemon_instance_id.clone(),
            })?;

        let budget = self.config.session_room_git_timeout();
        let measured_root = worktree_root.clone();
        let snapshot = tokio::task::spawn_blocking(move || {
            tddy_daemon_livekit::session_room::snapshot_worktree_within(&measured_root, budget)
        })
        .await
        .map_err(|e| Status::internal(format!("measuring {worktree_root:?} panicked: {e}")))?;

        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(&sessions_base, &req.session_id);
        let attachments = crate::session_attachments::list_session_attachments(&session_dir)
            .into_iter()
            .map(|a| a.basename)
            .collect();

        Ok(Response::new(GetWorktreeSnapshotResponse {
            head_commit: snapshot.head_commit,
            branch: snapshot.branch,
            changed_paths: snapshot.changed_paths,
            changed_files: snapshot.changed_files,
            lines_added: snapshot.lines_added,
            lines_removed: snapshot.lines_removed,
            untracked_files: snapshot.untracked_files,
            attachments,
        }))
    }

    async fn query_branch(
        &self,
        request: Request<QueryBranchRequest>,
    ) -> Result<Response<QueryBranchResponse>, Status> {
        use tddy_service::proto::connection::{
            BranchRemote, BranchResolution, BranchSession, BranchWorktree,
        };

        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.branch.trim().is_empty() {
            return Err(Status::invalid_argument("branch is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        // `None` when nothing in the session directory names a checkout: every repo-derived leg below
        // then reads as absent or unavailable, never as a confident answer about a repo we don't know.
        let repo_root = tddy_core::repo_root_for_session(&session_dir);
        let branch = req.branch.trim().to_string();

        // Session — the session that owns the branch, by the one rule `branch_owner` holds for every
        // surface that asks (prefer active, then most-recently-updated).
        let branch_for_scan = branch.clone();
        let sessions_base_for_scan = sessions_base.clone();
        let session = service_util::spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: scan sessions by branch",
            move || {
                crate::branch_owner::find_session_owning_branch(
                    &crate::session_reader::DaemonSessionListing,
                    &sessions_base_for_scan,
                    &branch_for_scan,
                )
            },
        )
        .await?;
        let session = match session {
            Some(s) => BranchSession {
                exists: true,
                session_id: s.session_id,
                is_active: s.is_active,
                status: s.status,
            },
            None => BranchSession::default(),
        };

        // Worktree — the on-disk worktree checked out for the branch (non-erroring), and whether it
        // holds outstanding work. Dirtiness is deliberately *not* cached with the base comparison
        // below: it is not a function of the two commits, and an operator's edit must show up on the
        // next tick.
        //
        // Both halves are git subprocesses — a `worktree list` walk and a `status --porcelain` — so
        // they run on the blocking pool like every other leg. A timeout degrades this leg to "no
        // worktree" rather than failing the call, which is the same contract the other four keep.
        let worktree_repo_root = repo_root.clone();
        let branch_for_worktree = branch.clone();
        let worktree = service_util::spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: read the branch's worktree",
            move || {
                Ok(worktree_leg(
                    worktree_repo_root.as_deref(),
                    &branch_for_worktree,
                ))
            },
        )
        .await
        .unwrap_or_else(|status| {
            log::warn!(
                "QueryBranch: the worktree leg did not complete: {}",
                status.message()
            );
            BranchWorktree::default()
        });

        // Remote — `origin/<branch>` in the orchestrator's repo, which is what a descendant's
        // worktree is created from. Only as fresh as the last fetch, so it can delay a spawn but
        // never permit one that would fail inside `git fetch`.
        //
        // Shells out to `git rev-parse`, so it goes to the blocking pool like every other leg here:
        // this handler is polled once per rendered row every few seconds, and a subprocess run
        // inline would occupy a runtime worker thread for its whole duration.
        let remote_repo_root = repo_root.clone();
        let remote_branch = branch.clone();
        let remote = service_util::spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: remote ref",
            move || {
                Ok(
                    match remote_repo_root.as_deref().and_then(|root| {
                        tddy_core::worktree::remote_branch_ref_sha(root, &remote_branch)
                    }) {
                        Some(sha) => BranchRemote { exists: true, sha },
                        None => BranchRemote::default(),
                    },
                )
            },
        )
        .await
        .unwrap_or_else(|status| {
            // Degrades this leg alone — an absent remote ref only blocks a *descendant's* spawn, and
            // reporting the RPC as failed would take the session, worktree and PR legs down with it.
            log::warn!(
                "QueryBranch: resolving the remote ref for '{branch}' did not complete: {}",
                status.message()
            );
            BranchRemote::default()
        });

        // Base sync — how the branch stands against the base the caller named. Like every other leg
        // it never fails the call: an unnamed base, an unknown checkout, a probe that could not run
        // and a probe that timed out all arrive as `unavailable` with a reason.
        let base_sync = self
            .base_sync_leg(repo_root.as_deref(), &branch, req.base_branch.trim())
            .await;

        // PR — same path as get_pr_status. A lookup that cannot be performed degrades this leg
        // alone; the session, worktree and remote legs above stay usable.
        let pr = self
            .pr_status_for_caller(&github_user, repo_root.as_deref(), &branch)
            .await;

        Ok(Response::new(QueryBranchResponse {
            resolution: Some(BranchResolution {
                branch,
                session: Some(session),
                worktree: Some(worktree),
                pr: Some(pr),
                remote: Some(remote),
                base_sync,
            }),
        }))
    }

    /// What a child spawn's branch bases off, answered by the daemon that owns the stack parent.
    ///
    /// The whole point of the call is *where* it is served. A `stack_parent` is a bare session id
    /// and the base is read out of that session's own `changeset.yaml`, so a daemon resolving one
    /// it does not hold reads an absent file — not "this branch was not planned", but "no such
    /// session", which refuses a spawn over an orchestrator that exists one host over. So the
    /// question is routed to the named daemon first, and answered from local disk only when this
    /// daemon is the one that owns it.
    ///
    /// Routed **before** the caller is authenticated, like the roster RPCs: a token is verified by
    /// the daemon that serves the call, and a peer's user mapping is not this one's to judge.
    async fn resolve_stack_base(
        &self,
        request: Request<ResolveStackBaseRequest>,
    ) -> Result<Response<ResolveStackBaseResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        if let Some(answered) = self
            .rpc_served_by_peer("ResolveStackBase", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let stack_parent = req.stack_parent.trim();
        if stack_parent.is_empty() {
            return Err(Status::invalid_argument(
                "stack_parent is required: ResolveStackBase resolves the base of a named parent session",
            ));
        }
        let os_user = self.resolve_os_user(&req.session_token)?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(&os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let projects_dir = projects_path_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        // This daemon's own checkout of the same logical project. The answer is a remote-tracking
        // ref, so the asking daemon's checkout and this one need only share an origin — never a path.
        let project = project_storage::find_project(&projects_dir, req.project_id.trim())
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        if !repo_root.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        let base_ref = tddy_core::resolve_chain_base_ref(
            &sessions_base,
            Some(stack_parent),
            &repo_root,
            req.stack_node_id.trim(),
            req.new_branch_name.trim(),
        )
        .map_err(Status::failed_precondition)?;
        log::info!(
            "ResolveStackBase: parent {stack_parent} in project {} bases {} on {:?}",
            req.project_id.trim(),
            req.new_branch_name.trim(),
            base_ref
        );
        Ok(Response::new(ResolveStackBaseResponse {
            base_ref: base_ref.unwrap_or_default(),
        }))
    }

    /// Record a child session and the branch it created on a planned node of a pr-stack
    /// orchestrator, answered by the daemon that owns that orchestrator.
    ///
    /// The write half of [`Self::resolve_stack_base`], and routed for the same reason: a planned
    /// node lives in its orchestrator's own `changeset.yaml`, so only the daemon holding that
    /// session can write it. A child spawned on another host writing its *own* sessions tree finds
    /// no such session and writes nothing at all — the node keeps neither a branch nor a child, and
    /// every descendant then stays unspawnable because
    /// [`tddy_core::changeset::Stack::base_ref_for_spawn`] gates on a parent owning a branch (D35).
    ///
    /// Routed **before** the caller is authenticated, like every other peer-routed RPC: a token is
    /// verified by the daemon that serves the call, and a peer's user mapping is not this one's to
    /// judge.
    ///
    /// Nothing here is guessed. An unnamed node is refused rather than derived from the branch
    /// (D34), and a node the plan does not hold is `not_found` with nothing written — silently
    /// succeeding against an unchanged plan is precisely the bug this RPC exists to remove.
    async fn link_stack_node(
        &self,
        request: Request<LinkStackNodeRequest>,
    ) -> Result<Response<LinkStackNodeResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        if let Some(answered) = self
            .rpc_served_by_peer("LinkStackNode", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let node_id = req.node_id.trim();
        if node_id.is_empty() {
            return Err(Status::invalid_argument(
                "node_id is required: a planned node is never derived from the branch a spawn creates, which the operator can rename before confirming",
            ));
        }
        let child_session_id = req.child_session_id.trim();
        if child_session_id.is_empty() {
            return Err(Status::invalid_argument(
                "child_session_id is required: a link names the session that materialized the node",
            ));
        }
        // Refused for the same reason `node_id` is. A blank branch leaves `node.branch` untouched —
        // `link_stack_node_to_child_session` takes `None` and writes nothing there — while the RPC
        // answers `Ok`, so the node still refuses every descendant and nothing said so. That is the
        // "reports success, changes nothing" shape D35 exists to remove, and it is reachable
        // whenever `effective_spawn_branch` yields an empty name.
        let branch = req.branch.trim();
        if branch.is_empty() {
            return Err(Status::invalid_argument(
                "branch is required: the branch is what makes the node's descendants spawnable, so a link that records none changes nothing",
            ));
        }

        let os_user = self.resolve_os_user(&req.session_token)?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(&os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.orchestrator_session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.orchestrator_session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        // Checked before the write, because `link_stack_node_to_child_session` leaves an unknown
        // node alone and reports success — which reads to the caller exactly like a link that
        // landed, and is the failure mode this whole path exists to make visible.
        let changeset =
            tddy_core::read_changeset(&session_dir).map_err(|e| Status::internal(e.to_string()))?;
        if changeset
            .stack
            .as_ref()
            .is_none_or(|stack| stack.node(node_id).is_none())
        {
            return Err(Status::not_found(format!(
                "planned node '{node_id}' is not in the stack of orchestrator {}",
                req.orchestrator_session_id.trim()
            )));
        }

        tddy_core::changeset::link_stack_node_to_child_session(
            &session_dir,
            node_id,
            child_session_id,
            Some(branch.to_string()),
        )
        .map_err(|e| {
            Status::internal(format!(
                "failed to link stack node '{node_id}' to child session {child_session_id}: {e}"
            ))
        })?;
        log::info!(
            "LinkStackNode: orchestrator {} node '{node_id}' now records child session {child_session_id} on branch '{branch}'",
            req.orchestrator_session_id.trim()
        );

        // Re-read and serialize through the same path `AddPlannedPr` / `RepointPlannedPr` /
        // `ReorderPlannedPr` answer with, so a caller that renders the plan reuses `parseStackPlan`
        // and does not re-read what this call already knows.
        let linked =
            tddy_core::read_changeset(&session_dir).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LinkStackNodeResponse {
            stack_plan_json: session_list_enrichment::stack_plan_json_for_changeset(&linked),
        }))
    }

    async fn repoint_planned_pr(
        &self,
        request: Request<RepointPlannedPrRequest>,
    ) -> Result<Response<RepointPlannedPrResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.node_id.trim().is_empty() {
            return Err(Status::invalid_argument("node_id is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        // A repoint rebases a local branch and re-targets a PR base, so it needs the real checkout:
        // an unknown one is a precondition failure, not a repoint attempted in the session directory.
        let repo_root = tddy_core::repo_root_for_session(&session_dir).ok_or_else(|| {
            Status::failed_precondition(
                "no checkout is recorded for this session, so its repository could not be resolved",
            )
        })?;
        let owner_repo = owner_repo_from_repo_root(&repo_root).unwrap_or_default();
        let default_branch =
            tddy_core::resolve_default_integration_base_ref(&repo_root).map_err(|e| {
                Status::failed_precondition(format!("could not resolve default branch: {e}"))
            })?;

        let node_id = req.node_id.trim().to_string();

        // On the wire, an unnamed target means "the project's default branch" — the daemon's own
        // resolved ref, substituted here.
        //
        // A client cannot always name it: `ProjectEntry.main_branch_ref` is empty for a project that
        // stores no default, and the web then renders "Repoint to default branch". Forwarding that
        // empty string would instead select the recipe's drop-merged-parents rule, which in the very
        // case this feature exists for — a predecessor whose PR merged but whose plan still records
        // `open` — drops nothing at all and returns success against an unchanged plan. The operator
        // would see no error and no change.
        //
        // The recipe's `None` mode is therefore in-process only; it is never reachable from here.
        let requested_target = if req.target_base_branch.trim().is_empty() {
            default_branch.as_str()
        } else {
            req.target_base_branch.as_str()
        };

        // The named target must be a branch this node can be based onto: the repoint retains only
        // the parents that own it, so an unvalidated target is a silent plan rewrite.
        let changeset =
            tddy_core::read_changeset(&session_dir).map_err(|e| Status::internal(e.to_string()))?;
        let stack = changeset.stack.unwrap_or_default();
        let parent_branches: Vec<String> = stack
            .node(&node_id)
            .map(|node| {
                node.parents
                    .iter()
                    .filter_map(|parent_id| stack.node(parent_id)?.branch.clone())
                    .collect()
            })
            .unwrap_or_default();
        let target_base_branch = hooks_and_urls::validate_repoint_target(
            requested_target,
            &default_branch,
            &parent_branches
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>(),
        )
        .map_err(Status::invalid_argument)?;

        let session_dir_for_op = session_dir.clone();
        tokio::task::spawn_blocking(move || {
            let gh = tddy_workflow_recipes::orchestrate_pr_stack::github::RealGithubPrApi::new(
                owner_repo,
            );
            tddy_workflow_recipes::pr_stack::repoint_planned_pr_node(
                &session_dir_for_op,
                &repo_root,
                &node_id,
                &default_branch,
                target_base_branch.as_deref(),
                &gh,
            )
        })
        .await
        .map_err(|e| Status::internal(format!("repoint_planned_pr join error: {e}")))?
        .map_err(Status::failed_precondition)?;

        // Re-read the just-updated changeset and reuse the same serializer as `ListSessions`
        // enrichment so the response's `stack_plan_json` is the exact wire shape `PrStackScreen`
        // already knows how to parse.
        let updated =
            tddy_core::read_changeset(&session_dir).map_err(|e| Status::internal(e.to_string()))?;
        let stack_plan_json = session_list_enrichment::stack_plan_json_for_changeset(&updated);
        Ok(Response::new(RepointPlannedPrResponse { stack_plan_json }))
    }

    /// Move one planned node up or down the operator's reading order.
    ///
    /// Touches nothing but `StackNode.display_order`: the dependency graph is a different fact, and
    /// keeping the two independent is the whole reason the field exists. Moving past either end is a
    /// successful no-op — the control at the end of the list is inert, not wrong.
    async fn reorder_planned_pr(
        &self,
        request: Request<ReorderPlannedPrRequest>,
    ) -> Result<Response<ReorderPlannedPrResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.node_id.trim().is_empty() {
            return Err(Status::invalid_argument("node_id is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        tddy_workflow_recipes::pr_stack::move_planned_pr_node(
            &session_dir,
            req.node_id.trim(),
            req.direction.trim(),
        )
        .map_err(Status::invalid_argument)?;

        // The same serializer `ListSessions` enrichment uses, so the response's `stack_plan_json` is
        // the exact wire shape `PrStackScreen`'s `parseStackPlan` already reads.
        let updated =
            tddy_core::read_changeset(&session_dir).map_err(|e| Status::internal(e.to_string()))?;
        let stack_plan_json = session_list_enrichment::stack_plan_json_for_changeset(&updated);
        Ok(Response::new(ReorderPlannedPrResponse { stack_plan_json }))
    }

    /// Take a base branch's commits into a planned node's branch, inside that node's own worktree,
    /// and push the result.
    ///
    /// A mutation, so an unknown checkout is a precondition failure rather than a degraded leg — the
    /// pull has nowhere to happen. The branch is then re-resolved **uncached**: the refs it compares
    /// have just moved, and the point of returning a resolution at all is that the row repaints
    /// without waiting for the next poll tick.
    async fn pull_base_into_branch(
        &self,
        request: Request<PullBaseIntoBranchRequest>,
    ) -> Result<Response<PullBaseIntoBranchResponse>, Status> {
        use tddy_service::proto::connection::{
            BranchRemote, BranchResolution, BranchSession, BranchWorktree,
        };

        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        if req.node_id.trim().is_empty() {
            return Err(Status::invalid_argument("node_id is required"));
        }
        if req.base_branch.trim().is_empty() {
            return Err(Status::invalid_argument("base_branch is required"));
        }
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        require_pr_stack_orchestrator(&session_dir)?;

        let repo_root = tddy_core::repo_root_for_session(&session_dir).ok_or_else(|| {
            Status::failed_precondition(
                "no checkout is recorded for this session, so its repository could not be resolved",
            )
        })?;

        let strategy = tddy_workflow_recipes::pr_stack::BaseSyncStrategy::from_wire(&req.strategy);
        let node_id = req.node_id.trim().to_string();
        let base_branch = req.base_branch.trim().to_string();
        let dirty_worktree_action = req.dirty_worktree_action.clone();
        let commit_message = req.commit_message.clone();
        let session_dir_for_op = session_dir.clone();
        let repo_root_for_op = repo_root.clone();
        let report = tokio::task::spawn_blocking(move || {
            tddy_workflow_recipes::pr_stack::pull_base_into_node_branch(
                &session_dir_for_op,
                &repo_root_for_op,
                &node_id,
                &base_branch,
                strategy,
                &dirty_worktree_action,
                &commit_message,
            )
        })
        .await
        .map_err(|e| Status::internal(format!("pull_base_into_branch join error: {e}")))?
        .map_err(Status::failed_precondition)?;

        // The branch the pull just moved, re-read from disk.
        let branch = tddy_core::read_changeset(&session_dir)
            .ok()
            .and_then(|changeset| changeset.stack)
            .and_then(|stack| stack.node(req.node_id.trim())?.branch.clone())
            .unwrap_or_default();

        // A sessions-directory walk, two worktree probes and a `git merge-tree` — all blocking, so
        // they go to the blocking pool together rather than tying up a runtime thread for the length
        // of four git subprocesses.
        let resolution_branch = branch.clone();
        let resolution_repo_root = repo_root.clone();
        let resolution_sessions_base = sessions_base.clone();
        let resolution_base_branch = req.base_branch.trim().to_string();
        let (session, worktree, remote, base_sync) = service_util::spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "PullBaseIntoBranch: re-read the branch",
            move || {
                let session = match crate::branch_owner::find_session_owning_branch(
                    &crate::session_reader::DaemonSessionListing,
                    &resolution_sessions_base,
                    &resolution_branch,
                )
                .ok()
                .flatten()
                {
                    Some(s) => BranchSession {
                        exists: true,
                        session_id: s.session_id,
                        is_active: s.is_active,
                        status: s.status,
                    },
                    None => BranchSession::default(),
                };
                let worktree = worktree_leg(Some(&resolution_repo_root), &resolution_branch);
                let remote = match tddy_core::worktree::remote_branch_ref_sha(
                    &resolution_repo_root,
                    &resolution_branch,
                ) {
                    Some(sha) => BranchRemote { exists: true, sha },
                    None => BranchRemote::default(),
                };
                // Uncached on purpose: the cache is keyed on the refs and the commits they point at,
                // and both commits may have just moved. Going through it would answer about the pair
                // the poll saw a moment ago.
                let base_sync = Some(
                    match tddy_core::base_sync::branch_base_sync(
                        &resolution_repo_root,
                        &resolution_branch,
                        &resolution_base_branch,
                    ) {
                        Ok(sync) => base_sync_view(sync),
                        Err(reason) => base_sync_unavailable(&resolution_base_branch, &reason),
                    },
                );
                Ok((session, worktree, remote, base_sync))
            },
        )
        .await
        // The pull itself already landed. Failing the whole call because re-reading the branch took
        // too long would leave the operator with no idea that it did, so the description degrades
        // and the report below still says what happened.
        .unwrap_or_else(|status| {
            let reason = format!(
                "the branch could not be re-read after the pull: {}",
                status.message()
            );
            log::warn!("PullBaseIntoBranch: {reason}");
            (
                BranchSession::default(),
                BranchWorktree::default(),
                BranchRemote::default(),
                Some(base_sync_unavailable(req.base_branch.trim(), &reason)),
            )
        });
        let pr = self
            .pr_status_for_caller(&github_user, Some(&repo_root), &branch)
            .await;

        Ok(Response::new(PullBaseIntoBranchResponse {
            resolution: Some(BranchResolution {
                branch,
                session: Some(session),
                worktree: Some(worktree),
                pr: Some(pr),
                remote: Some(remote),
                base_sync,
            }),
            strategy: report.strategy.to_string(),
            changed: report.changed,
            pushed: report.pushed,
            push_error: report.push_error.unwrap_or_default(),
        }))
    }

    /// Local peer-trust minting is not available on this transport. Peer credentials
    /// (SO_PEERCRED) exist only on the daemon's local Unix-domain socket; over ConnectRPC-HTTP or
    /// LiveKit there is no peer uid to trust, so those transports reach this tddy-rpc handler and
    /// are rejected. The UDS tonic adapter handles `MintLocalToken` itself and never delegates here.
    async fn mint_local_token(
        &self,
        _request: Request<MintLocalTokenRequest>,
    ) -> Result<Response<MintLocalTokenResponse>, Status> {
        Err(Status::unauthenticated(
            "local token minting is only available over the local socket",
        ))
    }

    /// Associated output stream type for [`stream_start_session`].
    type StreamStartSessionStream = MpscResultStream<StartSessionEvent>;

    async fn stream_start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<Self::StreamStartSessionStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        // Authenticate before classifying the route: forwarding opens an outbound RPC to a peer and
        // holds a pending-call slot on both hosts for the forward's whole deadline, so an
        // unauthenticated caller must never get that far. The resolved user is not used here — the
        // host that runs the session resolves it again under its own mapping — which is the same
        // order the unary `start_session` and `stream_read_host_document` already use.
        self.resolve_os_user(&req.session_token)?;

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "StreamStartSession: forwarding stream to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("StreamStartSession")?;
            // The session, its worktree and its attachments are created on the peer; only its
            // events cross back, so progress still reaches the client for the slowest case there
            // is — attachment bytes moving between two hosts.
            let rx = tddy_daemon_livekit::livekit_peer_discovery::forward_stream_start_session_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(MpscResultStream { rx }));
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<StartSessionEvent, Status>>();
        // The work runs on its own task so progress reaches the client while the host is still
        // materializing, rather than all at once after the start completes. A failure terminates
        // the stream with the status — a result event is only ever sent on success.
        let service = self.clone();
        let progress_tx = tx.clone();
        tokio::spawn(async move {
            let sink = AttachmentProgressSink::streaming(progress_tx);
            let event = match service.start_session_core(req, &sink).await {
                Ok(response) => Ok(StartSessionEvent {
                    event: Some(StartSessionEventKind::Result(response.into_inner())),
                }),
                Err(status) => Err(status),
            };
            let _ = tx.send(event);
        });
        Ok(Response::new(MpscResultStream { rx }))
    }
}
