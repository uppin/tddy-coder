// The parent module's own imports, carried in: this block was the whole `impl
// ConnectionServiceTrait` and reaches the same traits and helpers it always did. Unused
// entries are pruned below by the compiler's own spans.
use futures_util::StreamExt;
use tddy_terminal_rpc::TerminalSessionStore;
// `encode_to_vec` is a `prost::Message` method; the trait is imported anonymously because
// only its methods are used.
use super::stream_document_frames;
use crate::tool_engine;
use prost::Message as _;
use tddy_sandbox_runner::ExecuteToolResponse;
use tddy_service::proto::connection::start_session_event::Event as StartSessionEventKind;
use tddy_service::proto::connection::ConnectionService as ConnectionServiceTrait;
use tddy_service::proto::connection::SessionEntry as ProtoSessionEntry;
use tddy_service::proto::connection::{
    AcpReplayFrame, AddPlannedPrRequest, AddPlannedPrResponse, ClaimTerminalControlRequest,
    ClaimTerminalControlResponse, DeleteSessionUploadRequest, DeleteSessionUploadResponse,
    DeleteStagedAttachmentRequest, DeleteStagedAttachmentResponse, GetAcpReplayPageRequest,
    GetAcpReplayPageResponse, GetAcpToolCallDetailRequest, GetAcpToolCallDetailResponse,
    GetPrStatusRequest, GetPrStatusResponse, GetWorktreeSnapshotRequest,
    GetWorktreeSnapshotResponse, HostDocumentChunk, HostDocumentScope, LinkStackNodeRequest,
    LinkStackNodeResponse, ListSessionUploadsRequest, ListSessionUploadsResponse,
    ListStagedAttachmentsRequest, ListStagedAttachmentsResponse, LiveKitRoomsEvent,
    MintLocalTokenRequest, MintLocalTokenResponse, PullBaseIntoBranchRequest,
    PullBaseIntoBranchResponse, QueryBranchRequest, QueryBranchResponse, ReadHostDocumentRequest,
    ReadHostDocumentResponse, ReorderPlannedPrRequest, ReorderPlannedPrResponse,
    RepointPlannedPrRequest, RepointPlannedPrResponse, ResolveStackBaseRequest,
    ResolveStackBaseResponse, SessionNotificationEvent as ProtoSessionNotificationEvent,
    SessionUploadEntry, StagedAttachmentEntry, StartSessionEvent, StreamAcpReplayRequest,
    StreamLiveKitRoomsRequest, TerminalControlEvent, UploadSessionFileChunkRequest,
    UploadSessionFileChunkResponse, UploadStagedAttachmentChunkRequest,
    UploadStagedAttachmentChunkResponse, WatchTerminalControlRequest,
};
use tddy_service::proto::connection::{
    AgentActivityDeltaChunk, AgentActivityDeltaRequest, ExecuteToolRequest,
    ProjectEntry as ProtoProjectEntry,
};
use tddy_service::proto::connection::{
    AgentActivityRecord as ProtoAgentActivityRecord, StreamMode, StreamSessionNotificationsRequest,
};
use tddy_service::proto::connection::{
    DeltaScope as ProtoDeltaScope, ExecuteToolChunk, ListExecToolsRequest, ListExecToolsResponse,
    ListSessionToolCallsRequest, ListSessionToolCallsResponse, ListTerminalSessionsRequest,
    ListTerminalSessionsResponse, StartTerminalSessionResponse, StopTerminalSessionRequest,
    StopTerminalSessionResponse, TerminalSessionInfo,
};
use tddy_service::proto::connection::{
    DemoVmState, GetDemoVmStatusRequest, GetDemoVmStatusResponse, ReportAgentActivityRequest,
    ReportAgentActivityResponse, ReportSessionStatusRequest, ReportSessionStatusResponse,
    StartDemoVmRequest, StartDemoVmResponse, StopDemoVmRequest, StopDemoVmResponse,
    StreamSessionActivityRequest, ToolCallInfo as ProtoToolCallInfo,
};

use super::file_mtime_ms;

use crate::{
    connection_service::{
        activity_hub, agent_roster, hooks_and_urls, seed_codebase, seeded_clone_guard, service_util,
    },
    project_storage, session_deletion, session_list_enrichment, session_reader, spawn_worker,
    spawner,
};

use crate::livekit_rooms_stream::pump_rooms;

use super::MpscLiveKitRoomsStream;

use super::base_sync_unavailable;

use super::base_sync_view;

use super::owner_repo_from_repo_root;

use super::worktree_leg;

use super::require_pr_stack_orchestrator;

use super::relay_control_events;

use crate::cli_session_manager::ClaimOutcome;

use super::MpscControlEventStream;

use super::seq_by_tool_call;

use super::relay_acp_replay;

use super::acp_replay_frame;

use super::relay_acp_replay_count;

use super::MpscAcpReplayStream;

use super::relay_session_notifications;

use super::MpscSessionNotificationStream;

use super::MpscAgentActivityStream;

use super::exec_tool_result_frames;

use super::reject_exec_tool_path_traversal;

use tddy_service::proto::connection::ListProjectBranchesResponse;

use tddy_service::proto::connection::ListProjectBranchesRequest;

use tddy_service::proto::connection::StartTerminalSessionRequest;

use tddy_service::proto::connection::GetTerminalHistoryRequest;

use tddy_service::proto::connection::SendTerminalInputResponse;

use tddy_service::proto::connection::StreamTerminalOutputRequest;

use tddy_service::proto::connection::TerminalHistoryChunk;

use super::to_connection_output;

use super::to_bridge_terminal_input;

use super::TerminalFrameIdentity;

use tddy_service::proto::connection::SessionTerminalOutput;

use crate::cli_session_manager::MAIN_TERMINAL_ID;

use tddy_service::proto::connection::SessionTerminalInput;

use tddy_rpc::Streaming;

use super::MpscTerminalOutputStream;

use super::activity_delta_frames;

use crate::session_room::DeltaLookupError;

use crate::session_room::DeltaScope;

use tddy_service::proto::connection::ReadContextFileBatchRequest;

use tddy_service::proto::connection::ContextFileBatchChunk;

use tddy_service::proto::connection::ReadContextFileRequest;

use tddy_service::proto::connection::ContextFileChunk;

use tddy_service::proto::connection::ContextManifestRequest;

use tddy_service::proto::connection::ContextManifestEntry;

use tddy_service::proto::connection::ReadSessionWorkflowFileResponse;

use tddy_service::proto::connection::ReadSessionWorkflowFileRequest;

use tddy_service::proto::connection::WorkflowFileEntry;

use tddy_service::proto::connection::ListSessionWorkflowFilesResponse;

use tddy_service::proto::connection::ListSessionWorkflowFilesRequest;

use tddy_service::proto::connection::DeleteSessionResponse;

use tddy_service::proto::connection::DeleteSessionRequest;

use tddy_service::proto::connection::SignalSessionResponse;

use tddy_service::proto::connection::SignalSessionRequest;

use crate::spawner::SpawnOptions;

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

use super::agent_conversation_frames;

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
    async fn attach_session_agent(
        &self,
        request: Request<AttachSessionAgentRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a split session's roster is written where it is kept.
        if let Some(roster) = self
            .rpc_served_by_peer("AttachSessionAgent", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(roster));
        }

        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        let mut record = self.roster_record_for_agent_id(&req.agent_id).await?;
        let codebase = seed_codebase::SeedCodebase::read(&req.session_id, &session_dir)?;
        agent_roster::refuse_unenforceable_withdrawal(&req.session_id, &codebase, &record)?;
        // An agent owned by a peer reads a checkout on that peer, so the entry has to name one
        // before it is written. Claiming it is also what opens the session's room — and both happen
        // before the roster is touched, so an attach that cannot be completed leaves the session
        // looking exactly as it did (PRD § What attach does: "no roster entry, no half-built clone,
        // no room membership").
        let mut claimed = None;
        if record.daemon_instance_id != local_instance_id_for_config(&self.config) {
            let clone = self
                .claim_agent_clone(
                    &req.session_id,
                    &codebase,
                    &record.daemon_instance_id,
                    &req.session_token,
                )
                .await?;
            record.codebase_session_id = Some(clone.codebase_session_id.clone());
            claimed = Some((record.daemon_instance_id.clone(), clone));
        }
        // A roster this daemon could not write is an attach that did not happen, and the clone
        // claimed a moment ago is the half of it the peer has already been told to build: without
        // this the caller would be handed an error while a checkout it can no longer name kept being
        // cut on another host ("no roster entry, no half-built clone, no room membership").
        let roster = match self
            .session_agent_rosters
            .attach(&req.session_id, &session_dir, record)
        {
            Ok(roster) => roster,
            Err(e) => {
                if let Some((daemon_instance_id, clone)) = claimed {
                    self.unwind_agent_clone_claim(
                        &req.session_id,
                        &daemon_instance_id,
                        &clone,
                        &req.session_token,
                    )
                    .await;
                }
                return Err(e);
            }
        };
        self.broadcast_roster(&req.session_id, &roster).await;
        log::info!(
            "AttachSessionAgent: session {} holds {} agent(s) at rev {}",
            req.session_id,
            roster.agents.len(),
            roster.rev
        );
        Ok(Response::new(roster))
    }

    /// Detach one agent. An id the roster does not hold is `NOT_FOUND`, never a silent success.
    ///
    /// The entry is removed first and the checkout torn down after, in that order: a teardown that
    /// ran first and then failed to remove the entry would leave the roster naming a checkout that
    /// is gone, which is the state a prompt is served from.
    async fn detach_session_agent(
        &self,
        request: Request<DetachSessionAgentRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup: a detach served here leaves the entry standing on the daemon
        // whose roster actually holds it.
        if let Some(roster) = self
            .rpc_served_by_peer("DetachSessionAgent", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(roster));
        }

        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        let detached =
            self.session_agent_rosters
                .entry(&req.session_id, &session_dir, &req.agent_id)?;
        let roster =
            self.session_agent_rosters
                .detach(&req.session_id, &session_dir, &req.agent_id)?;
        self.cancel_conversations_with(&req.session_token, &req.session_id, &req.agent_id)
            .await;
        self.forget_agent_activity(&req.session_id, &req.agent_id);

        // The clone survives while another agent on that host still reads it — two agents on one
        // host share one checkout, so the last one out is what removes it.
        if let Some(record) = detached.filter(|r| r.codebase_session_id.is_some()) {
            let still_used = !self
                .session_agent_rosters
                .agents_owned_by(&req.session_id, &session_dir, &record.daemon_instance_id)?
                .is_empty();
            if !still_used {
                let codebase_session_id = record
                    .codebase_session_id
                    .clone()
                    .expect("filtered to entries naming a clone");
                // The entry is already gone and persisted by now, so the refusal says so: a message
                // that read as "the agent was left attached, retry" would send an operator into a
                // retry that answers NOT_FOUND while the checkout stays exactly where it is.
                if let Err(e) = self
                    .tear_down_agent_clone(
                        &req.session_id,
                        &record.daemon_instance_id,
                        &codebase_session_id,
                        &req.session_token,
                    )
                    .await
                {
                    self.broadcast_roster(&req.session_id, &roster).await;
                    return Err(Status {
                        code: e.code(),
                        message: format!(
                            "agent '{}' was detached from session '{}' (rev {}), but its clone \
                             could not be removed: {}. Retrying the detach reports NOT_FOUND — the \
                             checkout has to be deleted where it is.",
                            req.agent_id,
                            req.session_id,
                            roster.rev,
                            e.message()
                        ),
                    });
                }
            }
        }

        self.broadcast_roster(&req.session_id, &roster).await;
        log::info!(
            "DetachSessionAgent: session {} holds {} agent(s) at rev {}",
            req.session_id,
            roster.agents.len(),
            roster.rev
        );
        Ok(Response::new(roster))
    }

    async fn list_session_agents(
        &self,
        request: Request<ListSessionAgentsRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup: answered locally, a session that lives on another daemon reads
        // as one with no agents — an answer about the wrong host, indistinguishable from the truth.
        if let Some(roster) = self
            .rpc_served_by_peer("ListSessionAgents", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(roster));
        }

        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        Ok(Response::new(
            self.session_agent_rosters
                .snapshot(&req.session_id, &session_dir)?,
        ))
    }

    type StreamSessionAgentsStream = MpscResultStream<SessionAgentRoster>;

    /// The roster, now and on every change.
    ///
    /// The first frame is the current snapshot, taken with the subscription under one lock, so a
    /// late subscriber — the in-jail `tddy-tools` reconnecting, a browser tab opening — needs no
    /// separate priming read and cannot miss a change published between the two.
    async fn stream_session_agents(
        &self,
        request: Request<StreamSessionAgentsRequest>,
    ) -> Result<Response<Self::StreamSessionAgentsStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup. This is the call a split session's in-jail `tddy-tools` makes
        // first, and the daemon it addresses is not the one keeping the roster it subscribes to.
        if let Some(rx) = self
            .stream_served_by_peer("StreamSessionAgents", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(MpscResultStream { rx }));
        }

        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        let (snapshot, mut published) = self
            .session_agent_rosters
            .subscribe(&req.session_id, &session_dir)?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        // The roster this subscription has last sent, re-sent whenever the keepalive cadence elapses
        // with nothing published. See [`ROSTER_KEEPALIVE_INTERVAL`] for why a quiet roster still has
        // to talk. It tracks the last frame *sent* rather than the opening snapshot, so a subscriber
        // that reads only a keepalive is never told a superseded roster is the current one.
        let mut last_sent = snapshot.clone();
        if tx.send(Ok(snapshot)).is_err() {
            return Err(Status::internal(
                "StreamSessionAgents: the subscriber went away before its first frame",
            ));
        }
        let session_id = req.session_id.clone();
        let keepalive = self.roster_keepalive_interval;
        tokio::spawn(async move {
            loop {
                match tokio::time::timeout(keepalive, published.recv()).await {
                    Ok(Ok(roster)) => {
                        last_sent = roster.clone();
                        if tx.send(Ok(roster)).is_err() {
                            break;
                        }
                    }
                    // Every frame is a whole roster, so a subscriber that fell behind is brought
                    // fully current by the next one — the dropped frames carried nothing the
                    // survivor does not also carry.
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(missed))) => {
                        log::debug!(
                            "StreamSessionAgents: subscriber to session {session_id} fell {missed} \
                             snapshot(s) behind; the next one supersedes them"
                        );
                    }
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                    // Nothing changed for a whole cadence. Re-send, which also gives this task its
                    // only chance to notice a subscriber that went away: without a frame to fail on,
                    // it would park on a roster nobody changes for the life of the process.
                    Err(_) => {
                        if tx.send(Ok(last_sent.clone())).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Response::new(MpscResultStream { rx }))
    }

    /// Open a conversation with one roster agent.
    ///
    /// A local entry gets a turn loop in this process; a remote entry gets a routing record and the
    /// same call forwarded to its owning daemon. The caller cannot tell which happened, which is the
    /// property that makes remote agents usable at all (PRD AC28).
    ///
    /// A clone that is still being built refuses the open naming its state. Queuing it would make a
    /// 90-second `git clone` look like a hung agent, and serving it would read an empty checkout and
    /// report "not found" for a file that is simply not there yet (AC33).
    async fn open_agent_conversation(
        &self,
        request: Request<OpenAgentConversationRequest>,
    ) -> Result<Response<OpenAgentConversationResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup: the conversation is opened against the roster, so it opens on
        // the daemon holding it. Distinct from the forward further down, which follows the *agent's*
        // owning daemon once the roster entry naming it has been read.
        if let Some(opened) = self
            .rpc_served_by_peer("OpenAgentConversation", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(opened));
        }

        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        // Caller-chosen where offered, so an open that times out still leaves the caller able to
        // name — and therefore cancel — whatever this daemon built.
        let conversation_id = match req.conversation_id.trim().is_empty() {
            true => Uuid::now_v7().to_string(),
            false => req.conversation_id.trim().to_string(),
        };

        // This daemon *owns* the agent: the session is another daemon's, its roster is over there,
        // and the def is here. Resolving it against a roster this daemon does not hold would report
        // a session that legitimately is not here as the reason an agent it does own cannot answer.
        let conversation = match self.hosted_clone_for(&req.session_id) {
            Some(clone) => seed_codebase::AgentConversation::Local {
                session_id: req.session_id.clone(),
                agent_id: req.agent_id.clone(),
                session: Arc::new(tokio::sync::Mutex::new(
                    self.open_owned_agent_session(&req.agent_id, &clone).await?,
                )),
                closed: Arc::new(tokio::sync::Notify::new()),
            },
            None => {
                let record = self
                    .session_agent_rosters
                    .entry(&req.session_id, &session_dir, &req.agent_id)?
                    .ok_or_else(|| {
                        Status::invalid_argument(format!(
                            "agent '{}' is not attached to session '{}'",
                            req.agent_id, req.session_id
                        ))
                    })?;
                let local_instance_id = local_instance_id_for_config(&self.config);
                match record.daemon_instance_id == local_instance_id {
                    true => seed_codebase::AgentConversation::Local {
                        session_id: req.session_id.clone(),
                        agent_id: record.agent_id.clone(),
                        session: Arc::new(tokio::sync::Mutex::new(
                            self.open_local_agent_session(
                                &req.session_id,
                                &session_dir,
                                &record,
                                &req.session_token,
                            )
                            .await?,
                        )),
                        closed: Arc::new(tokio::sync::Notify::new()),
                    },
                    false => {
                        self.refuse_unready_clone(&req.session_id, &record)?;
                        self.refuse_departed_daemon(&record.daemon_instance_id)
                            .await?;
                        self.forward_open_agent_conversation(&req, &record, &conversation_id)
                            .await?;
                        seed_codebase::AgentConversation::Remote {
                            session_id: req.session_id.clone(),
                            agent_id: record.agent_id.clone(),
                            daemon_instance_id: record.daemon_instance_id.clone(),
                        }
                    }
                }
            }
        };
        self.agent_conversations
            .lock()
            .await
            .insert(conversation_id.clone(), conversation);
        // Open, not running: the conversation exists and has been asked nothing. This is also the
        // first moment an entry stops reporting UNSPECIFIED, which is what a reader needs to tell
        // "attached and reachable" from "attached, and this daemon has never heard from it".
        self.note_agent_activity(
            &req.session_id,
            &session_dir,
            &req.agent_id,
            crate::session_agent_status::ManagedAgentState::Open,
            "conversation opened",
        );
        Ok(Response::new(OpenAgentConversationResponse {
            conversation_id,
        }))
    }

    type PromptAgentConversationStream = MpscResultStream<AgentConversationChunk>;

    /// Prompt an open conversation, streaming the agent's answer back.
    ///
    /// Both variants end with exactly one `last` frame carrying the stop reason, so a consumer never
    /// has to distinguish "said nothing" from "nothing arrived", and a stream that ends without one
    /// was truncated rather than completed.
    async fn prompt_agent_conversation(
        &self,
        request: Request<PromptAgentConversationRequest>,
    ) -> Result<Response<Self::PromptAgentConversationStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup, and before the conversation map: a conversation opened on the
        // daemon holding the roster is not one this daemon can prompt, so served here it would report
        // "not open" for a conversation that is.
        if let Some(rx) = self
            .stream_served_by_peer("PromptAgentConversation", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(MpscResultStream { rx }));
        }

        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;

        // Everything the turn needs is taken out of the map here, under one lock, and the guard is
        // dropped before anything is awaited on it. The agent id comes out with it: the request
        // names a conversation, not an agent, and the status is recorded per agent.
        let (routing, agent_id) = {
            let open = self.agent_conversations.lock().await;
            match open.get(&req.conversation_id) {
                None => {
                    return Err(Status::not_found(format!(
                        "conversation '{}' is not open on session '{}'",
                        req.conversation_id, req.session_id
                    )))
                }
                Some(seed_codebase::AgentConversation::Local {
                    session,
                    closed,
                    agent_id,
                    ..
                }) => (
                    seeded_clone_guard::PromptRouting::Local {
                        session: Arc::clone(session),
                        closed: Arc::clone(closed),
                    },
                    agent_id.clone(),
                ),
                Some(seed_codebase::AgentConversation::Remote {
                    daemon_instance_id,
                    agent_id,
                    ..
                }) => (
                    seeded_clone_guard::PromptRouting::Remote(daemon_instance_id.clone()),
                    agent_id.clone(),
                ),
            }
        };

        // Stamped before either branch runs, so the badge changes when the turn starts rather than
        // when it is first observed to have started.
        self.note_agent_activity(
            &req.session_id,
            &session_dir,
            &agent_id,
            crate::session_agent_status::ManagedAgentState::Prompting,
            format!("prompted: {}", req.prompt),
        );

        let (session, closed) = match routing {
            seeded_clone_guard::PromptRouting::Local { session, closed } => (session, closed),
            seeded_clone_guard::PromptRouting::Remote(daemon_instance_id) => {
                let slot = self.common_room_slot("PromptAgentConversation")?;
                self.refuse_departed_daemon(&daemon_instance_id).await?;
                // Re-addressed to the agent's owning daemon, as the open was. Forwarded still naming
                // the daemon holding the roster, the peer would route it back here on that axis and
                // the two would hand the same turn to each other.
                let forwarded = PromptAgentConversationRequest {
                    daemon_instance_id: daemon_instance_id.clone(),
                    ..req.clone()
                };
                let rx = crate::livekit_peer_discovery::forward_server_stream_to_peer(
                    slot,
                    &daemon_instance_id,
                    "connection.ConnectionService",
                    "PromptAgentConversation",
                    forwarded.encode_to_vec(),
                    |bytes| {
                        AgentConversationChunk::decode(bytes.as_slice()).map_err(|e| {
                            Status::internal(format!(
                                "decode AgentConversationChunk from peer: {e}"
                            ))
                        })
                    },
                )
                .await?;
                // Relayed rather than handed straight back, for one reason: the roster this daemon
                // holds is what reports the status, and the end of the peer's stream is the only
                // moment this side learns the turn is over. Passed through unchanged — the caller
                // sees the peer's frames in the peer's order, errors included.
                return Ok(Response::new(MpscResultStream {
                    rx: self.relay_watching_for_the_turn_to_end(
                        rx,
                        &req.session_id,
                        &session_dir,
                        &agent_id,
                    ),
                }));
            }
        };

        // The turn loop runs here. Spawned rather than awaited so the stream's frames are produced
        // while the caller reads them, and awaited on the *conversation's* lock alone: two prompts on
        // one conversation are still serialized, but the map of open conversations is not held, so a
        // cancel can land while this turn is in flight — which is the only moment a cancel matters.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let conversation_id = req.conversation_id.clone();
        let prompt = req.prompt.clone();
        let turn_ended = self.turn_end_reporter(&req.session_id, &session_dir, &agent_id);
        tokio::spawn(async move {
            let outcome = tokio::select! {
                // Biased so a conversation already closed is reported as closed rather than racing
                // one more turn out of a model.
                biased;
                _ = closed.notified() => {
                    let _ = tx.send(Err(Status::failed_precondition(format!(
                        "conversation '{conversation_id}' was closed while its turn was in flight"
                    ))));
                    return;
                }
                outcome = async { session.lock().await.prompt(&prompt).await } => outcome,
            };
            match outcome {
                // Framed rather than sent whole: over LiveKit anything past MAX_CHUNK_FRAME_BYTES is
                // chunk-framed, and one lost chunk frame wedges the call with no error at all.
                Ok(outcome) => {
                    let content = outcome
                        .content
                        .iter()
                        .map(|block| block.text.as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    // Reported after the frames are on the wire, not before: a badge that drops to
                    // idle while the answer is still arriving is one a reader acts on too early.
                    let answered = format!("answered ({} chars)", content.chars().count());
                    for frame in agent_conversation_frames(
                        &content,
                        agent_roster::agent_stop_reason(outcome.stop_reason),
                    ) {
                        if tx.send(Ok(frame)).is_err() {
                            // The caller hung up mid-answer. The turn is over either way, and a
                            // badge left up would strand it.
                            turn_ended(answered);
                            return;
                        }
                    }
                    turn_ended(answered);
                }
                Err(e) => {
                    // Idle, not ERROR: the agent is still attached and still promptable, and it is
                    // the *clone* that ERROR is reserved for. The summary is what says what happened.
                    turn_ended(format!("turn failed: {e}"));
                    let _ = tx.send(Err(Status::internal(format!(
                        "agent conversation '{conversation_id}' failed: {e}"
                    ))));
                }
            }
        });
        Ok(Response::new(MpscResultStream { rx }))
    }

    /// Cancel an open conversation. An id nothing holds is `NOT_FOUND`, never a silent success — a
    /// caller told a turn was cancelled when it is still running would go on to read a stale answer.
    async fn cancel_agent_conversation(
        &self,
        request: Request<CancelAgentConversationRequest>,
    ) -> Result<Response<CancelAgentConversationResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup, and before the conversation map: a cancel that does not reach
        // the daemon the turn is running on cancels nothing while reporting that it did.
        if let Some(cancelled) = self
            .rpc_served_by_peer("CancelAgentConversation", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(cancelled));
        }

        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        let removed = self
            .agent_conversations
            .lock()
            .await
            .remove(&req.conversation_id);
        if let Some(conversation) = removed.as_ref() {
            // Back to "asked nothing", whichever daemon ran the loop: the conversation is gone, so
            // an agent left reporting a turn in flight would be one nothing can ever finish.
            self.note_agent_activity(
                &req.session_id,
                &session_dir,
                conversation.agent_id(),
                crate::session_agent_status::ManagedAgentState::NoConversation,
                "conversation cancelled",
            );
        }
        match removed {
            None => Err(Status::not_found(format!(
                "conversation '{}' is not open on session '{}'",
                req.conversation_id, req.session_id
            ))),
            Some(seed_codebase::AgentConversation::Local { closed, .. }) => {
                // A turn already in flight is interrupted rather than left to finish: the caller has
                // been told the conversation is cancelled, and an answer arriving afterwards would
                // be one it has no reason to expect.
                closed.notify_one();
                Ok(Response::new(CancelAgentConversationResponse {}))
            }
            Some(seed_codebase::AgentConversation::Remote {
                daemon_instance_id, ..
            }) => {
                let slot = self.common_room_slot("CancelAgentConversation")?;
                // Re-addressed to the agent's owning daemon, for the reason the prompt forward is: a
                // request still naming the daemon holding the roster would be routed back here.
                let forwarded = CancelAgentConversationRequest {
                    daemon_instance_id: daemon_instance_id.clone(),
                    ..req.clone()
                };
                crate::livekit_peer_discovery::forward_to_peer(
                    slot,
                    &daemon_instance_id,
                    "connection.ConnectionService",
                    "CancelAgentConversation",
                    forwarded.encode_to_vec(),
                )
                .await?;
                Ok(Response::new(CancelAgentConversationResponse {}))
            }
        }
    }

    /// The owning daemon telling this one how its clone is doing.
    ///
    /// Pushed rather than polled because only the daemon holding the checkout can say any of it, and
    /// accepted only for a clone this daemon actually asked that daemon for — the report is what
    /// authorizes an entry to start serving prompts.
    ///
    /// Authenticated first, and that is not ceremony: the (session, daemon, clone) triple the store
    /// matches on is published in the session's `session.agents` broadcast, so on the triple alone
    /// any participant that saw a roster frame could report a still-provisioning clone READY and
    /// have the next prompt served from an empty checkout.
    ///
    /// TODO(session-agent-roster): also bind the report to the *reporting participant*. The verified
    /// LiveKit participant identity is known at the transport but is not carried into
    /// `RequestMetadata` — `sender_identity` there is taken from the request envelope, which the
    /// sender writes itself, so checking `daemon_instance_id` against it would look like a check
    /// while refusing nothing.
    async fn report_agent_clone_state(
        &self,
        request: Request<tddy_service::proto::connection::ReportAgentCloneStateRequest>,
    ) -> Result<Response<tddy_service::proto::connection::ReportAgentCloneStateResponse>, Status>
    {
        self.record_rpc_activity();
        let req = request.into_inner();
        self.roster_session_dir(&req.session_token, &req.session_id)?;
        let state = tddy_service::proto::connection::AgentCloneState::try_from(req.clone_state)
            .unwrap_or(tddy_service::proto::connection::AgentCloneState::Unspecified);
        self.session_agent_clones
            .record_report(&crate::session_agent_clone::AgentCloneReport {
                session_id: req.session_id.clone(),
                daemon_instance_id: req.daemon_instance_id.clone(),
                codebase_session_id: req.codebase_session_id.clone(),
                state,
                error: req.clone_error.clone(),
                worktree_path: Some(req.worktree_path.clone())
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from),
                divergences: req.divergences.clone(),
            })?;
        log::info!(
            "ReportAgentCloneState: daemon {} reports session {}'s clone {} as {state:?}{}",
            req.daemon_instance_id,
            req.session_id,
            req.codebase_session_id,
            match req.divergences.len() {
                0 => String::new(),
                n => format!(" with {n} divergence(s)"),
            }
        );
        for divergence in &req.divergences {
            log::error!(
                "session {}'s clone on daemon {} diverged and was reconciled: {divergence}",
                req.session_id,
                req.daemon_instance_id
            );
        }
        let session_dir = self.session_dir_for(&req.session_id)?;
        self.publish_roster_change(&req.session_id, &session_dir)
            .await;
        Ok(Response::new(
            tddy_service::proto::connection::ReportAgentCloneStateResponse {},
        ))
    }

    /// An agent whose turn loop runs in the jail, telling this daemon what that loop is doing.
    ///
    /// The daemon infers a status from the conversation RPCs for every agent whose loop it runs. An
    /// agent the in-jail `tddy-tools` was *seeded* with runs its loop there instead, and this daemon
    /// is never asked to open anything — so without this report the row would sit at UNSPECIFIED for
    /// an agent that is demonstrably working.
    ///
    /// Three things are checked, and each is a way the roster could otherwise be made to lie:
    ///
    /// - **Routed first**, as the conversation RPCs are. The roster is on the facilitating daemon;
    ///   a report served anywhere else records a status nothing publishes.
    /// - **The agent must be attached.** An id the roster does not hold is `NOT_FOUND`, so an
    ///   in-jail registry that has gone stale cannot put a row on a roster an operator emptied.
    /// - **Only a conversation state is accepted.** `CONNECTING` and `ERROR` describe the checkout,
    ///   which this daemon measures itself and which outranks the conversation at snapshot time; a
    ///   reporter allowed to send them could hide a broken clone behind a cheerful conversation.
    ///
    /// Authenticated as `ReportAgentCloneState` is, and for the same reason: the (session, agent)
    /// pair is published in the `session.agents` broadcast, so on the pair alone any participant
    /// that saw a frame could park an agent at RUNNING for ever.
    async fn report_agent_conversation_state(
        &self,
        request: Request<tddy_service::proto::connection::ReportAgentConversationStateRequest>,
    ) -> Result<
        Response<tddy_service::proto::connection::ReportAgentConversationStateResponse>,
        Status,
    > {
        self.record_rpc_activity();
        let req = request.into_inner();

        if let Some(acknowledged) = self
            .rpc_served_by_peer(
                "ReportAgentConversationState",
                &req.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(acknowledged));
        }

        let session_dir = self.roster_session_dir(&req.session_token, &req.session_id)?;
        if self
            .session_agent_rosters
            .entry(&req.session_id, &session_dir, &req.agent_id)?
            .is_none()
        {
            return Err(Status::not_found(format!(
                "agent '{}' is not attached to session '{}', so there is no row to report on",
                req.agent_id, req.session_id
            )));
        }

        let status = tddy_service::proto::connection::SessionAgentStatus::try_from(req.status)
            .unwrap_or(tddy_service::proto::connection::SessionAgentStatus::Unspecified);
        let state = crate::session_agent_status::reported_state(status).ok_or_else(|| {
            Status::invalid_argument(format!(
                "{status:?} is not a conversation state: CONNECTING and ERROR describe the \
                 checkout, which this daemon measures itself, and UNSPECIFIED claims nothing"
            ))
        })?;

        self.note_agent_activity(
            &req.session_id,
            &session_dir,
            &req.agent_id,
            state,
            &req.summary,
        );
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

        match crate::supervisor_client::spawn_backend_choice(&self.config) {
            crate::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                service_util::await_supervised_with_timeout(
                    timeout,
                    "create_project: clone via tddy-supervisor",
                    crate::supervisor_spawn::clone_repo_via_supervisor(
                        &socket_path,
                        &os_user_owned,
                        &git_url_owned,
                        &dest_path,
                    ),
                )
                .await?
            }
            crate::supervisor_client::SpawnBackendChoice::ForkedWorker => {
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
            let inner = crate::livekit_peer_discovery::forward_add_project_to_host_via_livekit(
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

        match crate::supervisor_client::spawn_backend_choice(&self.config) {
            crate::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                service_util::await_supervised_with_timeout(
                    timeout,
                    "add_project_to_host: clone via tddy-supervisor",
                    crate::supervisor_spawn::clone_repo_via_supervisor(
                        &socket_path,
                        &os_user_owned,
                        &git_url_owned,
                        &dest_path,
                    ),
                )
                .await?
            }
            crate::supervisor_client::SpawnBackendChoice::ForkedWorker => {
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
                crate::livekit_peer_discovery::forward_set_project_default_branch_via_livekit(
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
        if crate::session_room::session_type_is_facilitated_here(session_type) {
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
        let result = match crate::supervisor_client::spawn_backend_choice(&self.config) {
            crate::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
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
                    crate::supervisor_spawn::spawn_session_via_supervisor(&socket_path, &spawn_req),
                )
                .await?
            }
            crate::supervisor_client::SpawnBackendChoice::ForkedWorker => {
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

    async fn list_session_workflow_files(
        &self,
        request: Request<ListSessionWorkflowFilesRequest>,
    ) -> Result<Response<ListSessionWorkflowFilesResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "ListSessionWorkflowFiles: session_id={}",
            req.session_id.trim()
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
        log::debug!(
            "ListSessionWorkflowFiles: resolved session_dir={:?}",
            session_dir
        );
        let basenames =
            crate::session_workflow_files::list_allowlisted_workflow_basenames(&session_dir)?;
        let n = basenames.len();
        let files: Vec<WorkflowFileEntry> = basenames
            .into_iter()
            .map(|basename| WorkflowFileEntry { basename })
            .collect();
        log::info!(
            "ListSessionWorkflowFiles: returning {} file(s) for session_id={}",
            n,
            req.session_id.trim()
        );
        Ok(Response::new(ListSessionWorkflowFilesResponse { files }))
    }

    async fn read_session_workflow_file(
        &self,
        request: Request<ReadSessionWorkflowFileRequest>,
    ) -> Result<Response<ReadSessionWorkflowFileResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "ReadSessionWorkflowFile: session_id={} basename={:?}",
            req.session_id.trim(),
            req.basename
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
        let content_utf8 = crate::session_workflow_files::read_allowlisted_workflow_file_utf8(
            &session_dir,
            &req.basename,
        )?;
        log::info!(
            "ReadSessionWorkflowFile: success session_id={} basename={:?} bytes={}",
            req.session_id.trim(),
            req.basename,
            content_utf8.len()
        );
        Ok(Response::new(ReadSessionWorkflowFileResponse {
            content_utf8,
        }))
    }

    type StreamContextManifestStream = MpscResultStream<ContextManifestEntry>;

    /// Every allow-listed path in a session's checkout, with the hash that says whether it moved —
    /// AC15-AC17 of `docs/ft/daemon/agent-context-sync.md`.
    ///
    /// Routed on `daemon_instance_id` before anything else, exactly as `GetWorktreeSnapshot` is and
    /// for the same reason: the caller is usually a split session's *agent* host asking the host
    /// that holds the codebase, and answered locally this would report the agent host's own empty
    /// session directory as the project's guidance.
    ///
    /// The gate is the compiled-in allow-list rather than git's listing — see
    /// [`crate::context_files`] for why that is safe and why it is necessary.
    async fn stream_context_manifest(
        &self,
        request: Request<ContextManifestRequest>,
    ) -> Result<Response<Self::StreamContextManifestStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        if let Some(rx) = self
            .stream_served_by_peer("StreamContextManifest", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(MpscResultStream { rx }));
        }

        let (sessions_base, worktree_root) =
            self.resolve_exec_tool_worktree(&ExecuteToolRequest {
                session_token: req.session_token.clone(),
                session_id: req.session_id.clone(),
                tool_name: "StreamContextManifest".to_string(),
                args_json: String::new(),
                daemon_instance_id: req.daemon_instance_id.clone(),
            })?;

        let globs = self.context_globs_for_session(
            "StreamContextManifest",
            &sessions_base,
            &req.session_id,
            &req.agent,
        )?;
        let max_bytes = self.config.max_attachment_bytes;
        let timeout = self.config.spawn_worker_request_timeout();
        // Hashing every allow-listed file is filesystem work, so it runs off the async runtime the
        // way the neighbouring readers' does, and it can fail the call outright — which is why it
        // happens here rather than inside the stream, where a refusal would have to be told apart
        // from a mid-stream error.
        let join = tokio::task::spawn_blocking(move || {
            crate::context_files::context_manifest(&worktree_root, globs, max_bytes)
        });
        let entries = match tokio::time::timeout(timeout, join).await {
            Ok(Ok(Ok(entries))) => entries,
            Ok(Ok(Err(status))) => return Err(status),
            Ok(Err(join_err)) => return Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => {
                let secs = timeout.as_secs();
                return Err(Status::deadline_exceeded(format!(
                    "StreamContextManifest: timed out after {secs}s \
                     (spawn_worker_request_timeout_secs)"
                )));
            }
        };

        let (tx, rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<ContextManifestEntry, Status>>();
        for entry in entries {
            if tx.send(Ok(entry)).is_err() {
                break;
            }
        }
        Ok(Response::new(MpscResultStream { rx }))
    }

    type StreamReadContextFileStream = MpscResultStream<ContextFileChunk>;

    /// The bytes of one allow-listed path — AC18-AC22 of `docs/ft/daemon/agent-context-sync.md`.
    ///
    /// Same addressing, same routing and the same allow-list gate as
    /// [`Self::stream_context_manifest`]; what differs is that this one carries content, framed at
    /// [`crate::context_files::CONTEXT_FILE_FRAME_BYTES`] so it never engages the transport's
    /// chunking codec. An over-cap file is refused **before the first frame** rather than shortened,
    /// because a truncated `CLAUDE.md` silently drops the project's last rule and nothing
    /// downstream could tell.
    async fn stream_read_context_file(
        &self,
        request: Request<ReadContextFileRequest>,
    ) -> Result<Response<Self::StreamReadContextFileStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        if let Some(rx) = self
            .stream_served_by_peer("StreamReadContextFile", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(MpscResultStream { rx }));
        }

        let (sessions_base, worktree_root) =
            self.resolve_exec_tool_worktree(&ExecuteToolRequest {
                session_token: req.session_token.clone(),
                session_id: req.session_id.clone(),
                tool_name: "StreamReadContextFile".to_string(),
                args_json: String::new(),
                daemon_instance_id: req.daemon_instance_id.clone(),
            })?;

        let globs = self.context_globs_for_session(
            "StreamReadContextFile",
            &sessions_base,
            &req.session_id,
            &req.agent,
        )?;
        let rel_path = req.rel_path.clone();
        let max_bytes = self.config.max_attachment_bytes;
        let timeout = self.config.spawn_worker_request_timeout();
        let join = tokio::task::spawn_blocking(move || {
            crate::context_files::read_context_file_bytes(
                &worktree_root,
                &rel_path,
                globs,
                max_bytes,
            )
        });
        let bytes = match tokio::time::timeout(timeout, join).await {
            Ok(Ok(Ok(bytes))) => bytes,
            Ok(Ok(Err(status))) => return Err(status),
            Ok(Err(join_err)) => return Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => {
                let secs = timeout.as_secs();
                return Err(Status::deadline_exceeded(format!(
                    "StreamReadContextFile: timed out after {secs}s \
                     (spawn_worker_request_timeout_secs)"
                )));
            }
        };

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<ContextFileChunk, Status>>();
        for frame in crate::context_files::context_file_frames(&bytes) {
            if tx.send(Ok(frame)).is_err() {
                break;
            }
        }
        Ok(Response::new(MpscResultStream { rx }))
    }

    type StreamReadContextFileBatchStream = MpscResultStream<ContextFileBatchChunk>;

    /// The bytes of several allow-listed paths in one call — the setup sync's prefetch.
    ///
    /// Everything about the single-file read applies unchanged: the same routing, the same
    /// session-derived allow-list, the same `spawn_blocking` and timeout, the same 48 KiB framing,
    /// and the same refusals before the first frame. What differs is the round-trip count, and that
    /// is the entire point. Populating a split session's context directory reads every allow-listed
    /// path *before* the agent process exists, and one call per file made a 120-file
    /// `.claude/skills/` tree into 121 sequential peer calls: around eighteen seconds of dead time
    /// on a 150 ms link and 121 separate chances to trip `PEER_FORWARD_TIMEOUT`, where an ordinary
    /// split start used to make no extra peer calls at all.
    ///
    /// **One refusal fails the whole batch**, before any bytes are read
    /// ([`crate::context_files::read_context_files_bytes`]). Serving what it can would leave the
    /// caller unable to tell "the project does not ship that file" from "this host would not serve
    /// it", and setup sync must fail loudly rather than start an agent against guidance with a hole
    /// in it.
    async fn stream_read_context_file_batch(
        &self,
        request: Request<ReadContextFileBatchRequest>,
    ) -> Result<Response<Self::StreamReadContextFileBatchStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        if let Some(rx) = self
            .stream_served_by_peer("StreamReadContextFileBatch", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(MpscResultStream { rx }));
        }

        let (sessions_base, worktree_root) =
            self.resolve_exec_tool_worktree(&ExecuteToolRequest {
                session_token: req.session_token.clone(),
                session_id: req.session_id.clone(),
                tool_name: "StreamReadContextFileBatch".to_string(),
                args_json: String::new(),
                daemon_instance_id: req.daemon_instance_id.clone(),
            })?;

        let globs = self.context_globs_for_session(
            "StreamReadContextFileBatch",
            &sessions_base,
            &req.session_id,
            &req.agent,
        )?;
        let rel_paths = req.rel_paths.clone();
        let max_bytes = self.config.max_attachment_bytes;
        let timeout = self.config.spawn_worker_request_timeout();
        let join = tokio::task::spawn_blocking(move || {
            crate::context_files::read_context_files_bytes(
                &worktree_root,
                &rel_paths,
                globs,
                max_bytes,
            )
        });
        let files = match tokio::time::timeout(timeout, join).await {
            Ok(Ok(Ok(files))) => files,
            Ok(Ok(Err(status))) => return Err(status),
            Ok(Err(join_err)) => return Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => {
                let secs = timeout.as_secs();
                return Err(Status::deadline_exceeded(format!(
                    "StreamReadContextFileBatch: timed out after {secs}s \
                     (spawn_worker_request_timeout_secs)"
                )));
            }
        };

        let (tx, rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<ContextFileBatchChunk, Status>>();
        for frame in crate::context_files::context_file_batch_frames(&files) {
            if tx.send(Ok(frame)).is_err() {
                break;
            }
        }
        Ok(Response::new(MpscResultStream { rx }))
    }

    type StreamAgentActivityDeltaStream = MpscResultStream<AgentActivityDeltaChunk>;

    /// The tick delta lookup — AC6-AC14 of `docs/ft/daemon/session-worktree-sync.md`.
    ///
    /// The delta lives in the session room's store, which is why this is answered from the room
    /// registry rather than from disk: a patch is a measurement of a live checkout, and the daemon
    /// hosting that room is the only one that took it.
    ///
    /// Authorization comes **first**, before the store is even looked up, for the reason AC14
    /// gives: an unauthenticated caller must not be able to learn which sessions this daemon hosts
    /// by reading apart a `NOT_FOUND` from a `PERMISSION_DENIED`.
    async fn stream_agent_activity_delta(
        &self,
        request: Request<AgentActivityDeltaRequest>,
    ) -> Result<Response<Self::StreamAgentActivityDeltaStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        self.resolve_os_user(&req.session_token)?;

        let call_id = req.call_id.trim();
        if call_id.is_empty() {
            return Err(Status::invalid_argument(
                "call_id is required; there is no whole-worktree delta",
            ));
        }

        // A room this daemon does not host has no measurement of that checkout and never will, so
        // this is an absence rather than a failure — named, so a client can tell "wrong daemon"
        // from "unknown call".
        let store = self
            .session_rooms
            .delta_store(&req.session_id)
            .ok_or_else(|| {
                Status::not_found(format!(
                    "no session room is hosted here for session {}, so it has no deltas",
                    req.session_id
                ))
            })?;

        let scope = match ProtoDeltaScope::try_from(req.scope).unwrap_or(ProtoDeltaScope::Call) {
            ProtoDeltaScope::Call => DeltaScope::Call,
            ProtoDeltaScope::Residual => DeltaScope::Residual,
            ProtoDeltaScope::Tick => DeltaScope::Tick,
        };

        let delta = {
            let store = store
                .lock()
                .map_err(|_| Status::internal("session delta store is poisoned"))?;
            store.delta_for_call(call_id, scope)
        };

        // Both variants are NOT_FOUND and both carry a distinct message, because the client's
        // response differs: an unknown call is a defect to report, an aged-out delta is an ordinary
        // reconcile from the WIP ref. One shared message would make a long mirror's routine
        // recovery indistinguishable from a bug on one side or the other.
        let delta = match delta {
            Ok(delta) => delta,
            Err(DeltaLookupError::UnknownCall { call_id }) => {
                return Err(Status::not_found(format!(
                    "unknown call {call_id}: this daemon has no record of it in session {}",
                    req.session_id
                )))
            }
            Err(DeltaLookupError::AgedOut { call_id, seq }) => {
                return Err(Status::not_found(format!(
                    "delta for call {call_id} (tick {seq}) has aged out of this session's ring; reconcile from the WIP ref"
                )))
            }
        };

        let (tx, rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<AgentActivityDeltaChunk, Status>>();
        for frame in activity_delta_frames(&delta) {
            if tx.send(Ok(frame)).is_err() {
                break;
            }
        }
        Ok(Response::new(MpscResultStream { rx }))
    }

    /// Associated output stream type for [`stream_session_terminal_io`].
    type StreamSessionTerminalIoStream = MpscTerminalOutputStream;

    async fn stream_session_terminal_io(
        &self,
        request: Request<Streaming<SessionTerminalInput>>,
    ) -> Result<Response<Self::StreamSessionTerminalIoStream>, Status> {
        let mut in_stream = request.into_inner();

        // Read the first message to get session_id and session_token for auth.
        let first: SessionTerminalInput = in_stream
            .next()
            .await
            .ok_or_else(|| Status::invalid_argument("stream ended before first message"))?
            .map_err(|e| Status::internal(e.to_string()))?;

        let github_user = (self.user_resolver)(&first.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = first.session_id.clone();
        let terminal_id = service_util::resolved_terminal_id(&first.terminal_id).to_string();
        log::info!(
            target: "tddy_daemon::connection_service",
            "stream_session_terminal_io: session_id={} terminal_id={}",
            session_id,
            terminal_id
        );

        if let Some(sandbox) = self.sandbox_manager.get(&session_id).await {
            if terminal_id != MAIN_TERMINAL_ID {
                return Err(Status::not_found("terminal not found or not running"));
            }
            let stdin_tx = sandbox.stdin_tx.clone();
            if !first.data.is_empty() {
                let _ = stdin_tx.send(bytes::Bytes::from(first.data));
            }
            // Sandbox bidi path stays live-only (no capture-ring replay on this surface): forward
            // subsequent input chunks to stdin, and bridge the sandbox stdout broadcast into the
            // mpsc-backed stream the tonic/RpcService trait drains.
            let stdin_tx2 = stdin_tx.clone();
            tokio::spawn(async move {
                while let Some(Ok(msg)) = in_stream.next().await {
                    if !msg.data.is_empty() {
                        let _ = stdin_tx2.send(bytes::Bytes::from(msg.data));
                    }
                }
            });
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SessionTerminalOutput>();
            let mut stdout_rx = sandbox.stdout_tx.subscribe();
            let identity = TerminalFrameIdentity::new(&session_id, &terminal_id);
            tokio::spawn(async move {
                loop {
                    match stdout_rx.recv().await {
                        Ok(chunk) => {
                            if tx.send(identity.data_frame(chunk.to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            return Ok(Response::new(MpscTerminalOutputStream { rx }));
        }

        if !self
            .claude_cli_manager
            .verify_control(&session_id, &first.control_token)
            .await
        {
            return Err(Status::failed_precondition(
                "terminal controlled by another screen",
            ));
        }

        let store = crate::terminal_session_adapter::DaemonTerminalSessionStore::new(Arc::clone(
            &self.claude_cli_manager,
        ));
        let session = store
            .get_terminal(&session_id, &terminal_id)
            .await
            .ok_or_else(|| {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "stream_session_terminal_io: session {} terminal {} not found in registry",
                    session_id,
                    terminal_id
                );
                Status::not_found("terminal not found or not running")
            })?;

        // Convert the first (open) message and the remaining tonic input stream into the bridge's
        // `terminal_session` proto types so the bidi handler can route through the shared bridge
        // helper (same replay-once / resume-by-offset semantics as the split `StreamTerminalOutput`).
        let first_bridge = to_bridge_terminal_input(&first);
        let in_stream_mapped = tokio_stream::StreamExt::map(in_stream, |item| match item {
            Ok(msg) => Ok(to_bridge_terminal_input(&msg)),
            Err(e) => Err(tddy_rpc::Status::internal(e.to_string())),
        });

        // Per-chunk control-token verifier (the first message was already verified above). The
        // bridge bidi helper calls this on each subsequent input chunk and ends the forwarder when
        // control is lost — matching the daemon's previous per-chunk control-token check.
        let manager_for_verify = Arc::clone(&self.claude_cli_manager);
        let verify_control = move |sid: &str, token: &str| {
            let manager = Arc::clone(&manager_for_verify);
            let sid = sid.to_string();
            let token = token.to_string();
            async move { manager.verify_control(&sid, &token).await }
        };

        let bridge_rx = tddy_terminal_rpc::serve_stream_session_terminal_io_with(
            session,
            session_id,
            first_bridge,
            in_stream_mapped,
            verify_control,
            tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
        )
        .await?;

        // Map the bridge's `terminal_session::SessionTerminalOutput` frames (carrying offset
        // metadata) into the daemon's `connection::SessionTerminalOutput` and forward them through
        // the mpsc-backed stream the tonic/RpcService trait drains.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SessionTerminalOutput>();
        tokio::spawn(async move {
            let mut bridge_rx = bridge_rx;
            while let Some(frame) = bridge_rx.recv().await {
                let mapped = match frame {
                    Ok(out) => to_connection_output(out),
                    Err(_) => break,
                };
                if tx.send(mapped).is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(MpscTerminalOutputStream { rx }))
    }

    /// Associated output stream type for [`stream_terminal_output`].
    type StreamTerminalOutputStream = MpscTerminalOutputStream;

    /// Associated output stream type for [`get_terminal_history`].
    type GetTerminalHistoryStream = MpscResultStream<TerminalHistoryChunk>;

    /// Server-streaming output — browser-compatible alternative to the bidi `StreamSessionTerminalIO`.
    /// connect-web's Fetch transport cannot send streaming request bodies, so bidi streaming never
    /// reaches the daemon from a browser. This RPC provides the output half; input goes via the
    /// unary `SendTerminalInput`.
    async fn stream_terminal_output(
        &self,
        request: Request<StreamTerminalOutputRequest>,
    ) -> Result<Response<Self::StreamTerminalOutputStream>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        let terminal_id = service_util::resolved_terminal_id(&req.terminal_id).to_string();
        log::info!(
            target: "tddy_daemon::connection_service",
            "stream_terminal_output: session_id={} terminal_id={}",
            session_id,
            terminal_id
        );

        if let Some(sandbox) = self.sandbox_manager.get(&session_id).await {
            if terminal_id != MAIN_TERMINAL_ID {
                return Err(Status::not_found("terminal not found or not running"));
            }
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            // Every frame this stream emits names the session and terminal it came from, so a client
            // rendering another terminal drops it instead of painting it.
            let identity = TerminalFrameIdentity::new(&session_id, &terminal_id);

            // Re-issue the mouse-tracking modes still in effect as the very first frame (not part of
            // the cumulative byte stream, so zeroed offsets), then forward-fill the retained buffer
            // from the appropriate offset. TAIL (first connect) clamps `from_offset = 0` up to the
            // ring's `start_offset`, replaying the full retained buffer; FROM_OFFSET (reconnect)
            // replays only the gap from the client's tracked offset to the tip. Each chunk is tagged
            // with its absolute offsets so the client advances its `currentOffset` to the tip and can
            // resume by offset on the next reconnect — no duplicate replay.
            let is_from_offset =
                req.mode == tddy_service::proto::connection::StreamReplayMode::FromOffset as i32;
            let from_offset = if is_from_offset { req.from_offset } else { 0 };

            let prologue = sandbox
                .capture
                .lock()
                .map(|cap| cap.mode_prologue())
                .unwrap_or_default();
            if !prologue.is_empty() {
                let _ = tx.send(identity.data_frame(prologue));
            }

            // `from_offset` is clamped DOWN to the tip: a client whose cumulative counter drifted
            // ahead of the stream would otherwise be handed its own bogus offset back and would keep
            // asking for bytes the capture will never hold. Exactly one offset-anchored frame is
            // always emitted (an empty one tagged with the tip when there is no gap), so every open
            // SETS the client's cumulative offset instead of leaving it to be inferred from the
            // frames that carry none — matching `tddy_terminal_rpc::bridge`.
            let tip = sandbox
                .capture
                .lock()
                .map(|cap| cap.end_offset())
                .unwrap_or_default();
            let mut cursor = from_offset.min(tip);
            let mut anchored = false;
            loop {
                let chunk = sandbox
                    .capture
                    .lock()
                    .map(|cap| {
                        cap.replay_from(cursor, 0, service_util::TERMINAL_OUTPUT_FRAME_MAX_BYTES)
                    })
                    .unwrap_or_else(|_| tddy_task::CaptureChunk {
                        data: Vec::new(),
                        start_offset: cursor,
                        end_offset: cursor,
                        at_oldest: true,
                        at_end: true,
                    });
                let (end_offset, at_end) = (chunk.end_offset, chunk.at_end);
                if !chunk.data.is_empty() || !anchored {
                    let _ = tx.send(identity.replay_frame(
                        chunk.data,
                        chunk.start_offset,
                        chunk.end_offset,
                        chunk.at_oldest,
                    ));
                    anchored = true;
                }
                cursor = end_offset;
                if at_end {
                    break;
                }
            }

            let mut stdout_rx = sandbox.stdout_tx.subscribe();
            // Sandbox sessions have no unary input-offset ACK source; data frames only.
            tokio::spawn(async move {
                loop {
                    match stdout_rx.recv().await {
                        Ok(chunk) => {
                            if tx.send(identity.data_frame(chunk.to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            return Ok(Response::new(MpscTerminalOutputStream { rx }));
        }

        let store = crate::terminal_session_adapter::DaemonTerminalSessionStore::new(Arc::clone(
            &self.claude_cli_manager,
        ));
        // Delegate the claude-cli terminal stream to the unified bridge in `tddy-terminal-rpc`, which
        // sends the mode prologue + current last frame first (tagged with absolute offsets), resizes
        // and drains on client dimensions, emits the current ACK up front, then bridges live
        // broadcast output interleaved with ACKs until the child exits. Older history is fetched on
        // demand via `get_terminal_history` as the user scrolls up. The bridge resolves an empty
        // `terminal_id` to the reserved main terminal, matching the daemon's `resolved_terminal_id`.
        let bridge_req = tddy_terminal_rpc::proto::terminal_session::StreamTerminalOutputRequest {
            session_token: req.session_token.clone(),
            session_id: req.session_id.clone(),
            terminal_id: req.terminal_id.clone(),
            initial_cols: req.initial_cols,
            initial_rows: req.initial_rows,
            mode: req.mode,
            from_offset: req.from_offset,
        };
        let bridge_rx = tddy_terminal_rpc::serve_stream_terminal_output_with(
            &store,
            bridge_req,
            tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
        )
        .await?;

        // Convert the bridge's `terminal_session::SessionTerminalOutput` frames (which carry the
        // offset metadata) into the daemon's `connection::SessionTerminalOutput` and forward them
        // through the mpsc-backed stream the tonic/RpcService trait drains. The bridge only ever
        // emits `Ok` frames after opening (open errors are surfaced via the `await?` above), so an
        // `Err` here just ends the stream.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SessionTerminalOutput>();
        tokio::spawn(async move {
            let mut bridge_rx = bridge_rx;
            while let Some(frame) = bridge_rx.recv().await {
                let mapped = match frame {
                    Ok(out) => to_connection_output(out),
                    Err(_) => break,
                };
                if tx.send(mapped).is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(MpscTerminalOutputStream { rx }))
    }

    /// Unary input — browser-compatible alternative to the client-streaming half of `StreamSessionTerminalIO`.
    async fn send_terminal_input(
        &self,
        request: Request<SessionTerminalInput>,
    ) -> Result<Response<SendTerminalInputResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        let terminal_id = service_util::resolved_terminal_id(&req.terminal_id).to_string();

        if let Some(sandbox) = self.sandbox_manager.get(&session_id).await {
            if terminal_id != MAIN_TERMINAL_ID {
                return Err(Status::not_found("terminal not found or not running"));
            }
            if !req.data.is_empty() {
                let _ = sandbox.stdin_tx.send(bytes::Bytes::from(req.data));
            }
            return Ok(Response::new(SendTerminalInputResponse {}));
        }

        if !self
            .claude_cli_manager
            .verify_control(&session_id, &req.control_token)
            .await
        {
            return Err(Status::failed_precondition(
                "terminal controlled by another screen",
            ));
        }

        let handle = self
            .claude_cli_manager
            .get_terminal(&session_id, &terminal_id)
            .await
            .ok_or_else(|| Status::not_found("terminal not found or not running"))?;

        if !req.data.is_empty() {
            log::trace!(
                target: "tddy_daemon::connection_service",
                "send_terminal_input: session_id={} terminal_id={} {} bytes: {:?}",
                session_id,
                terminal_id,
                req.data.len(),
                String::from_utf8_lossy(&req.data)
            );
            let input_offset = req.input_offset;
            handle.send_input(bytes::Bytes::from(req.data), input_offset);
        }
        Ok(Response::new(SendTerminalInputResponse {}))
    }

    /// `GetTerminalHistory`: lazy scroll-up — one chunk of older output ending just before the
    /// request's `before_offset`, then the stream closes. Delegates to the unified bridge in
    /// `tddy-terminal-rpc` over a [`DaemonTerminalSessionStore`]. Sandbox sessions have no capture
    /// ring wired here, so they report `not_found` (the sandbox path streams its own replay).
    async fn get_terminal_history(
        &self,
        request: Request<GetTerminalHistoryRequest>,
    ) -> Result<Response<Self::GetTerminalHistoryStream>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        if self.sandbox_manager.get(&session_id).await.is_some() {
            return Err(Status::not_found("terminal not found or not running"));
        }

        let store = crate::terminal_session_adapter::DaemonTerminalSessionStore::new(Arc::clone(
            &self.claude_cli_manager,
        ));
        let bridge_req = tddy_terminal_rpc::proto::terminal_session::GetTerminalHistoryRequest {
            session_token: req.session_token.clone(),
            session_id: req.session_id.clone(),
            terminal_id: req.terminal_id.clone(),
            from_offset: req.from_offset,
            until_offset: req.until_offset,
            max_bytes: req.max_bytes,
        };
        let bridge_rx = tddy_terminal_rpc::serve_get_terminal_history_with(
            &store,
            bridge_req,
            tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
        )
        .await?;

        let (tx, rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<TerminalHistoryChunk, Status>>();
        tokio::spawn(async move {
            let mut bridge_rx = bridge_rx;
            while let Some(frame) = bridge_rx.recv().await {
                let mapped = frame.map(|chunk| TerminalHistoryChunk {
                    data: chunk.data,
                    start_offset: chunk.start_offset,
                    end_offset: chunk.end_offset,
                    at_oldest: chunk.at_oldest,
                    at_end: chunk.at_end,
                });
                if tx.send(mapped).is_err() {
                    break;
                }
            }
        });
        Ok(Response::new(MpscResultStream { rx }))
    }

    async fn start_terminal_session(
        &self,
        request: Request<StartTerminalSessionRequest>,
    ) -> Result<Response<StartTerminalSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        // A Bash tool runs in the session's worktree, resolved from the main (claude) terminal.
        let main = self
            .claude_cli_manager
            .get(&session_id)
            .await
            .ok_or_else(|| Status::failed_precondition("session has no running terminal"))?;
        let worktree = main.worktree_path.clone();

        // The Bash tool is built-in: the target user's passwd login shell (not the daemon's own
        // `$SHELL`, which under systemd/nix is not the user's interactive shell), falling back to
        // `$SHELL`, then /bin/bash.
        let shell = crate::pty_runtime::login_shell_for_os_user(os_user)
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/bash".to_string());

        let handle = self
            .claude_cli_manager
            .start_terminal(&session_id, worktree, &shell)
            .await
            .map_err(|e| Status::internal(format!("failed to start terminal: {e}")))?;

        Ok(Response::new(StartTerminalSessionResponse {
            terminal_id: handle.terminal_id.clone(),
        }))
    }

    async fn stop_terminal_session(
        &self,
        request: Request<StopTerminalSessionRequest>,
    ) -> Result<Response<StopTerminalSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        let terminal_id = req.terminal_id.trim().to_string();
        if terminal_id == MAIN_TERMINAL_ID {
            return Err(Status::invalid_argument(
                "the main terminal cannot be stopped via StopTerminalSession; \
                 use SignalSession or DeleteSession",
            ));
        }

        if self
            .claude_cli_manager
            .stop_terminal(&session_id, &terminal_id)
            .await
        {
            Ok(Response::new(StopTerminalSessionResponse {
                ok: true,
                message: String::new(),
            }))
        } else {
            Err(Status::not_found("terminal not found"))
        }
    }

    async fn list_terminal_sessions(
        &self,
        request: Request<ListTerminalSessionsRequest>,
    ) -> Result<Response<ListTerminalSessionsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let session_id = req.session_id.trim().to_string();
        let terminals = self
            .claude_cli_manager
            .list_terminals(&session_id)
            .await
            .iter()
            .map(|h| TerminalSessionInfo {
                terminal_id: h.terminal_id.clone(),
                kind: h.kind.clone(),
                pid: h.pid,
            })
            .collect();
        Ok(Response::new(ListTerminalSessionsResponse { terminals }))
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
            let rx = crate::livekit_peer_discovery::forward_stream_execute_tool_via_livekit(
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

        // Validate session_id segment to prevent path traversal.
        tddy_core::validate_session_id_segment(&req.session_id)
            .map_err(|_| Status::invalid_argument("invalid session_id"))?;

        // Validate status string before any IO.
        tddy_core::SessionActivityStatus::from_wire(&req.status)
            .ok_or_else(|| Status::invalid_argument(format!("unknown status: {}", req.status)))?;

        // Resolve sessions_base from os_user (no web session token available for hooks).
        let sessions_base = crate::user_sessions_path::sessions_base_for_user(
            &req.os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::not_found("unknown os_user or sessions_base not found"))?;

        let session_dir = tddy_core::unified_session_dir_path(&sessions_base, &req.session_id);

        // Read session metadata — not found if the directory/yaml doesn't exist.
        let meta = tddy_core::read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        // claude-cli and cursor-cli sessions support hook status reporting.
        let session_type = meta.session_type.as_deref().unwrap_or("");
        if session_type != "claude-cli" && session_type != "cursor-cli" {
            return Err(Status::failed_precondition(
                "session_type is not claude-cli or cursor-cli",
            ));
        }

        // Validate hook_token (constant-time string comparison acceptable here — local process).
        let stored_token = meta.hook_token.as_deref().unwrap_or("");
        if stored_token != req.hook_token {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        // Persist the activity status.
        tddy_core::update_activity_status(&session_dir, &req.status)
            .map_err(|e| Status::internal(format!("failed to update activity status: {}", e)))?;

        log::debug!(
            target: "tddy_daemon::connection_service",
            "report_session_status: session={} status={}",
            req.session_id,
            req.status
        );

        // One publish, every interested subscriber: Telegram renders the attention-worthy ones,
        // and the notification stream carries all of them to the drawer's indicators. A subscriber
        // that fails is logged by the bus and never fails this hook (PRD NFR3).
        //
        // The notification names `req.os_user` as its owner — the same user whose sessions
        // directory the hook token was just checked against — so the stream can hand it to that
        // operator's clients and to no one else's.
        if let Some(ref bus) = self.session_notification_bus {
            let label = crate::session_notifications::resolve_session_label(
                &sessions_base,
                &req.session_id,
            );
            if let Some(notification) =
                crate::session_notifications::notification_for_activity_status(
                    &req.session_id,
                    &req.os_user,
                    &label,
                    &req.status,
                    tddy_daemon_kernel::now_unix_ms(),
                )
            {
                bus.publish(notification).await;
            }
        }

        Ok(Response::new(ReportSessionStatusResponse { ok: true }))
    }

    async fn report_agent_activity(
        &self,
        request: Request<ReportAgentActivityRequest>,
    ) -> Result<Response<ReportAgentActivityResponse>, Status> {
        let req = request.into_inner();

        // Validate session_id segment to prevent path traversal.
        tddy_core::validate_session_id_segment(&req.session_id)
            .map_err(|_| Status::invalid_argument("invalid session_id"))?;

        // Resolve sessions_base from os_user (no web session token available for hooks).
        let sessions_base = crate::user_sessions_path::sessions_base_for_user(
            &req.os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::not_found("unknown os_user or sessions_base not found"))?;

        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        // Read session metadata — not found if the directory/yaml doesn't exist.
        let meta = tddy_core::read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        // Validate hook_token (local-process comparison; the token is a per-session secret).
        let stored_token = meta.hook_token.as_deref().unwrap_or("");
        if stored_token != req.hook_token {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        // AC1/AC2 of `docs/ft/daemon/session-worktree-sync.md`: the record names the commit it was
        // made against and the paths it declared, so a consumer holding a patch can place it. The
        // HEAD is read from the filesystem rather than by spawning `git rev-parse` — an agent makes
        // a great many tool calls, and a subprocess on each would be paid on every one of them.
        //
        // A session with no checkout on this host stamps neither: `read_head_commit` returns an
        // empty string when HEAD cannot be resolved, and a path has nothing to be relative to. That
        // is the honest answer AC1 asks for, and the reason no sha is invented in its place.
        let worktree_root = meta.repo_path.as_deref().map(PathBuf::from);
        let head_commit = worktree_root
            .as_deref()
            .map(tddy_core::git_head::read_head_commit)
            .unwrap_or_default();
        let input = tddy_core::agent_activity::parse_activity_json(&req.input_json);
        let changed_paths = worktree_root
            .as_deref()
            .map(|root| tddy_core::agent_activity::declared_paths(&req.tool_name, &input, root))
            .unwrap_or_default();

        let record = match req.event.as_str() {
            "PreToolUse" => {
                // A tool call started: mint a call_id, remember it so the paired PostToolUse can
                // reuse it, and append the `running` row.
                let call_id = Uuid::new_v4().to_string();
                self.agent_activity_hub
                    .push_pending(&req.session_id, &call_id);
                tddy_core::agent_activity::AgentActivityRecord {
                    call_id,
                    tool_name: req.tool_name,
                    input,
                    status: tddy_core::agent_activity::STATUS_RUNNING.to_string(),
                    result: serde_json::Value::Null,
                    error_message: String::new(),
                    started_unix_ms: tddy_daemon_kernel::now_unix_ms(),
                    completed_unix_ms: 0,
                    source: "claude-cli".to_string(),
                    head_commit,
                    // The tick that covers this call has not been measured yet; the poll loop
                    // attributes it when it runs. `0` is the wire's "no tick has covered it yet".
                    activity_seq: 0,
                    changed_paths,
                }
            }
            "PostToolUse" => {
                // The tool call finished: pair with the most-recent pending call_id (fresh id when
                // none is outstanding, e.g. a hook restart), and append the terminal row.
                let call_id = self
                    .agent_activity_hub
                    .pop_pending(&req.session_id)
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                let status = if req.is_error {
                    tddy_core::agent_activity::STATUS_ERROR
                } else {
                    tddy_core::agent_activity::STATUS_COMPLETED
                };
                tddy_core::agent_activity::AgentActivityRecord {
                    call_id,
                    tool_name: req.tool_name,
                    input,
                    status: status.to_string(),
                    result: tddy_core::agent_activity::parse_activity_json(&req.result_json),
                    error_message: req.error_message,
                    started_unix_ms: 0,
                    completed_unix_ms: tddy_daemon_kernel::now_unix_ms(),
                    source: "claude-cli".to_string(),
                    head_commit,
                    // As on the `running` row: the covering tick is the poll loop's to attribute.
                    activity_seq: 0,
                    changed_paths,
                }
            }
            other => {
                return Err(Status::invalid_argument(format!(
                    "unknown event: {other} (expected PreToolUse or PostToolUse)"
                )));
            }
        };

        // The durable log is the source of truth; a write failure must not fail the hook call.
        if let Err(e) = tddy_core::agent_activity::append_agent_activity(&session_dir, &record) {
            log::warn!(
                "agent_activity: failed to persist {} for session {}: {}",
                req.event,
                req.session_id,
                e
            );
        }
        // The agent's own tool loop is the other thing that means "this session is working", and
        // the only one a cursor-cli or tool session reports at all. Owned by `req.os_user`, as at
        // the activity-status site above: the notification stream relays it to that operator only.
        if let Some(ref bus) = self.session_notification_bus {
            let label = crate::session_notifications::resolve_session_label(
                &sessions_base,
                &req.session_id,
            );
            bus.publish(
                crate::session_notifications::notification_for_agent_tool_call(
                    &req.session_id,
                    &req.os_user,
                    &label,
                    &record.tool_name,
                    tddy_daemon_kernel::now_unix_ms(),
                ),
            )
            .await;
        }
        self.agent_activity_hub.publish(&req.session_id, record);
        // The record is **not** broadcast into the session room from here, deliberately.
        //
        // A record announced at this point names a tick nothing has measured yet: its
        // `activity_seq` is still `0` and the delta covering its files is produced by the next poll
        // tick, so a participant that reacted to it and asked for the call's delta would be told
        // `UnknownCall` — an announcement that arrives before the thing it announces.
        //
        // The poll loop is the single broadcaster instead, tailing `agent-activity.jsonl`, which is
        // also what makes cursor-cli and tool sessions visible: their agents never call this RPC at
        // all, and a room fed only from here would carry claude-cli activity and nothing else.
        Ok(Response::new(ReportAgentActivityResponse { ok: true }))
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

    type StreamSessionActivityStream = MpscAgentActivityStream;

    /// Stream a session's agent activity: replay the persisted `agent-activity.jsonl` snapshot,
    /// then relay live records published to the hub for this session.
    async fn stream_session_activity(
        &self,
        request: Request<StreamSessionActivityRequest>,
    ) -> Result<Response<Self::StreamSessionActivityStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay can forward. A request addressed to a remote daemon
        // is rejected rather than silently served from the local (wrong) log.
        // TODO(agent-activity): forward StreamSessionActivity to a peer daemon over
        // `forward_server_stream_to_peer`. The primitive exists and carries an idle deadline sized
        // for a short-lived stream; this one is long-lived and open-ended, so migrating it needs a
        // keepalive frame (or a per-call deadline) first — otherwise an idle session's activity
        // stream would be terminated as a stalled peer.
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
                    log::info!("StreamSessionActivity: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    return Err(Status::unimplemented(format!(
                        "StreamSessionActivity forwarding to remote daemon_instance_id={peer_instance_id} is not supported yet"
                    )));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Authenticate caller (same path as list_session_tool_calls).
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ProtoAgentActivityRecord>();

        // Snapshot-then-live (the default and proto3 zero value) replays the coalesced on-disk
        // records first, then relays everything subsequently published to the hub for this
        // session. Live-only skips the snapshot entirely and carries only records published after
        // subscribe.
        let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);
        if mode == StreamMode::SnapshotThenLive {
            let snapshot =
                tddy_core::agent_activity::read_agent_activity(&session_dir).unwrap_or_default();
            for record in snapshot {
                if tx
                    .send(tddy_service::agent_activity_to_proto(record))
                    .is_err()
                {
                    // Receiver already gone — return an empty live stream that terminates immediately.
                    return Ok(Response::new(MpscAgentActivityStream { rx }));
                }
            }
        }

        let broadcast_rx = self.agent_activity_hub.subscribe(&req.session_id);
        tokio::spawn(activity_hub::relay_agent_activity(broadcast_rx, tx));

        Ok(Response::new(MpscAgentActivityStream { rx }))
    }

    // --- session notifications ---

    type StreamSessionNotificationsStream = MpscSessionNotificationStream;

    /// Stream every session notification this daemon raises, for as long as the client stays
    /// connected.
    ///
    /// Daemon-level by design (PRD NFR1): the request names no session, because one subscription
    /// serves a drawer of any size. It does *not* name a user either — the caller's own token
    /// does, and the relay carries only the sessions belonging to the OS user it maps to.
    /// Live-only: each event carries the moment it happened, and a replayed backlog would raise
    /// indicators for turns that finished while the tab was closed.
    async fn stream_session_notifications(
        &self,
        request: Request<StreamSessionNotificationsRequest>,
    ) -> Result<Response<Self::StreamSessionNotificationsStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Authenticate, then authorize exactly as `stream_session_activity` does: a token that
        // maps to no OS user owns no sessions on this host, so there is nothing it may be shown.
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?
            .to_string();

        let broadcast_rx = self
            .session_notification_bus
            .as_ref()
            .and_then(|bus| bus.subscribe_clients())
            .ok_or_else(|| {
                Status::failed_precondition("this daemon publishes no session notifications")
            })?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ProtoSessionNotificationEvent>();
        tokio::spawn(relay_session_notifications(broadcast_rx, tx, os_user));

        Ok(Response::new(MpscSessionNotificationStream { rx }))
    }

    // --- ACP transcript replay ---

    type StreamAcpReplayStream = MpscAcpReplayStream;

    /// Stream a session's read-only ACP transcript: replay the session's resolved transcript
    /// snapshot (`acp-transcript.jsonl` merged with the durable `agent-activity.jsonl` — see
    /// [`tddy_service::acp_replay::read_session_transcript`]), then relay live agent-activity records
    /// (mapped to ACP `tool_call` frames) published to the hub for this session. Mirrors
    /// [`stream_session_activity`] — same routing, auth, and [`StreamMode`] semantics.
    async fn stream_acp_replay(
        &self,
        request: Request<StreamAcpReplayRequest>,
    ) -> Result<Response<Self::StreamAcpReplayStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay can forward. A request addressed to a remote daemon
        // is rejected rather than silently served from the local (wrong) transcript.
        // TODO(acp-replay): forward StreamAcpReplay to a peer daemon over
        // `forward_server_stream_to_peer`, blocked on the same keepalive gap as
        // `stream_session_activity` above — this stream stays open for a session's whole life, and
        // the primitive's idle deadline is sized for a short-lived one.
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
                    log::info!("StreamAcpReplay: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    return Err(Status::unimplemented(format!(
                        "StreamAcpReplay forwarding to remote daemon_instance_id={peer_instance_id} is not supported yet"
                    )));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Local) => {
                    // Fall through to local execution below.
                }
            }
        }

        // Authenticate caller (same path as stream_session_activity).
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<AcpReplayFrame>();

        // Snapshot-then-live (the default and proto3 zero value) replays the persisted transcript
        // first, then relays everything subsequently published to the hub for this session.
        // Live-only skips the snapshot entirely and carries only frames produced after subscribe.
        let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);

        // Count-first mode emits only the running count of persisted transcript frames — one frame
        // now with the current count, then a fresh count each time a record is published — with no
        // transcript payload. It never replays the snapshot itself.
        if mode == StreamMode::CountThenLive {
            let snapshot =
                tddy_service::acp_replay::read_session_transcript(&session_dir).unwrap_or_default();
            let count = tddy_service::acp_replay::count_activity_entries(&snapshot);
            let seen_ids = tddy_service::acp_replay::tool_call_ids(&snapshot);
            if tx
                .send(AcpReplayFrame {
                    acp_agent_message: Vec::new(),
                    activity_count: count,
                    // A count frame carries no transcript payload, so it has no position.
                    seq: 0,
                })
                .is_err()
            {
                // Receiver already gone — return an empty live stream that terminates immediately.
                return Ok(Response::new(MpscAcpReplayStream { rx }));
            }
            let broadcast_rx = self.agent_activity_hub.subscribe(&req.session_id);
            tokio::spawn(relay_acp_replay_count(broadcast_rx, tx, count, seen_ids));
            return Ok(Response::new(MpscAcpReplayStream { rx }));
        }

        // The resolved transcript is what every position refers to: the replayed frames index into
        // it, and the live tail continues its numbering from the end of it. Live-only replays none
        // of it but still needs its length, so a live frame's `seq` means the same thing there.
        let snapshot =
            tddy_service::acp_replay::read_session_transcript(&session_dir).unwrap_or_default();

        // Which slice of the transcript is replayed on subscribe, and where in the transcript that
        // slice starts: all of it (snapshot-then-live, the proto3 default), its newest page only
        // (tail-then-live), or none of it (live-only).
        let (first_seq, replayed): (u64, &[tddy_service::proto::acp::AcpAgentMessage]) = match mode
        {
            StreamMode::SnapshotThenLive => (0, &snapshot),
            StreamMode::TailThenLive => {
                let page = tddy_service::acp_replay::tail_page(
                    &snapshot,
                    usize::try_from(req.page_size).unwrap_or(usize::MAX),
                );
                (page.first_seq, page.frames)
            }
            _ => (0, &[]),
        };
        for (offset, frame) in replayed.iter().enumerate() {
            if tx
                .send(acp_replay_frame(frame, first_seq + offset as u64))
                .is_err()
            {
                // Receiver already gone — return an empty live stream that terminates immediately.
                return Ok(Response::new(MpscAcpReplayStream { rx }));
            }
        }

        let broadcast_rx = self.agent_activity_hub.subscribe(&req.session_id);
        tokio::spawn(relay_acp_replay(
            broadcast_rx,
            tx,
            snapshot.len() as u64,
            seq_by_tool_call(&snapshot),
        ));

        Ok(Response::new(MpscAcpReplayStream { rx }))
    }

    /// Return one tool call's full `raw_input`/`raw_output` from the session's coalesced transcript
    /// (the bodies `stream_acp_replay` strips out). Mirrors `stream_acp_replay`'s routing/auth and
    /// maps an unknown `tool_call_id` to `NOT_FOUND`.
    async fn get_acp_tool_call_detail(
        &self,
        request: Request<GetAcpToolCallDetailRequest>,
    ) -> Result<Response<GetAcpToolCallDetailResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay (which has no local sessions) can forward.
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
                    log::info!("GetAcpToolCallDetail: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "GetAcpToolCallDetail: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward GetAcpToolCallDetail: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "connection.ConnectionService",
                        "GetAcpToolCallDetail",
                        body,
                    )
                    .await?;
                    let inner =
                        GetAcpToolCallDetailResponse::decode(out.as_slice()).map_err(|e| {
                            Status::internal(format!("decode GetAcpToolCallDetailResponse: {e}"))
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

        // Validate session ID.
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        // Resolve the session dir.
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        let detail = tddy_service::acp_replay::tool_call_detail(&session_dir, &req.tool_call_id)
            .map_err(|e| Status::internal(format!("read transcript: {e}")))?;
        match detail {
            None => Err(Status::not_found(format!(
                "no tool call with id {} in session {}",
                req.tool_call_id, req.session_id
            ))),
            Some(detail) => Ok(Response::new(GetAcpToolCallDetailResponse {
                raw_input: detail.raw_input,
                raw_output: detail.raw_output,
            })),
        }
    }

    /// Return one page of transcript frames strictly older than `before_seq` — the reverse cursor a
    /// tail-first replay pages backwards with. Mirrors [`get_acp_tool_call_detail`]'s routing (it
    /// peer-forwards, unlike the streaming modes) and `stream_acp_replay`'s auth, and applies the
    /// same `strip_tool_body` seam the replay stream does: a paged frame is not a back door to the
    /// bodies.
    async fn get_acp_replay_page(
        &self,
        request: Request<GetAcpReplayPageRequest>,
    ) -> Result<Response<GetAcpReplayPageResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route BEFORE session lookup so a relay (which has no local sessions) can forward.
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
                    log::info!("GetAcpReplayPage: rejected daemon routing: {}", msg);
                    return Err(Status::invalid_argument(msg));
                }
                Ok(crate::livekit_peer_discovery::PeerRoute::Forward { peer_instance_id }) => {
                    log::info!(
                        "GetAcpReplayPage: forwarding RPC to remote daemon_instance_id={}",
                        peer_instance_id
                    );
                    let slot = self.common_room_livekit_room.as_ref().ok_or_else(|| {
                        Status::failed_precondition(
                            "cannot forward GetAcpReplayPage: this process has no LiveKit common-room connection",
                        )
                    })?;
                    let body = req.encode_to_vec();
                    let out = crate::livekit_peer_discovery::forward_to_peer(
                        slot,
                        &peer_instance_id,
                        "connection.ConnectionService",
                        "GetAcpReplayPage",
                        body,
                    )
                    .await?;
                    let inner = GetAcpReplayPageResponse::decode(out.as_slice()).map_err(|e| {
                        Status::internal(format!("decode GetAcpReplayPageResponse: {e}"))
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

        // Validate session ID.
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        // Resolve the session dir.
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        // A transcript that cannot be read is an error, never an empty page: an empty page means
        // "you have reached the head", and a reader told that stops paging for good.
        let transcript = tddy_service::acp_replay::read_session_transcript(&session_dir)
            .map_err(|e| Status::internal(format!("read transcript: {e}")))?;
        let page = tddy_service::acp_replay::page_before(
            &transcript,
            req.before_seq,
            usize::try_from(req.page_size).unwrap_or(usize::MAX),
        );

        Ok(Response::new(GetAcpReplayPageResponse {
            frames: page
                .frames
                .iter()
                .map(|frame| tddy_service::acp_replay::strip_tool_body(frame).encode_to_vec())
                .collect(),
            first_seq: page.first_seq,
            at_oldest: page.at_oldest,
        }))
    }

    // --- terminal control mutex ---

    type WatchTerminalControlStream = MpscControlEventStream;

    /// Claim exclusive input control of a session's terminals.
    async fn claim_terminal_control(
        &self,
        request: Request<ClaimTerminalControlRequest>,
    ) -> Result<Response<ClaimTerminalControlResponse>, Status> {
        let req = request.into_inner();
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let outcome = self
            .claude_cli_manager
            .claim_control(&req.session_id, &req.screen_id, req.steal)
            .await;
        let resp = match outcome {
            ClaimOutcome::Granted { control_token } => ClaimTerminalControlResponse {
                granted: true,
                control_token,
                current_holder_screen_id: String::new(),
            },
            ClaimOutcome::Denied { holder_screen_id } => ClaimTerminalControlResponse {
                granted: false,
                control_token: String::new(),
                current_holder_screen_id: holder_screen_id,
            },
        };
        Ok(Response::new(resp))
    }

    /// Watch for control-lease changes on a session; emits a snapshot immediately, then one event
    /// per lease change.
    async fn watch_terminal_control(
        &self,
        request: Request<WatchTerminalControlRequest>,
    ) -> Result<Response<Self::WatchTerminalControlStream>, Status> {
        let req = request.into_inner();
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let session_id = req.session_id.clone();
        let control_token = req.control_token.clone();

        let you_are_controller = self
            .claude_cli_manager
            .verify_control(&session_id, &control_token)
            .await;
        let holder_screen_id = self
            .claude_cli_manager
            .current_control(&session_id)
            .await
            .map(|l| l.holder_screen_id)
            .unwrap_or_default();

        let broadcast_rx = self.claude_cli_manager.subscribe_control();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<TerminalControlEvent>();

        let snapshot = TerminalControlEvent {
            holder_screen_id,
            you_are_controller,
        };
        let _ = tx.send(snapshot);

        let manager = Arc::clone(&self.claude_cli_manager);
        tokio::spawn(relay_control_events(
            session_id,
            control_token,
            manager,
            broadcast_rx,
            tx,
        ));

        Ok(Response::new(MpscControlEventStream { rx }))
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
            crate::session_room::snapshot_worktree_within(&measured_root, budget)
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

    type StreamLiveKitRoomsStream = MpscLiveKitRoomsStream;

    /// Stream the LiveKit server's rooms and their participants: one full snapshot, then one change
    /// event per delta found by polling the room service.
    ///
    /// Authenticates `session_token`, then spawns [`pump_rooms`], which emits the snapshot
    /// immediately and re-reads the roster on the poll cadence, diffing each read against the state
    /// **this** stream was last sent — a per-subscriber baseline, so two watchers cannot consume
    /// each other's deltas. A tick with no delta emits nothing, so an idle server yields an idle
    /// stream. The task ends when the receiver is dropped (client unsubscribe), and a roster read
    /// that fails ends the stream with that error rather than reporting an empty server.
    async fn stream_live_kit_rooms(
        &self,
        request: Request<StreamLiveKitRoomsRequest>,
    ) -> Result<Response<Self::StreamLiveKitRoomsStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<LiveKitRoomsEvent, Status>>();
        tokio::spawn(pump_rooms(
            Arc::clone(&self.room_roster),
            self.room_poll_interval,
            tx,
        ));

        Ok(Response::new(MpscLiveKitRoomsStream { rx }))
    }

    async fn upload_session_file_chunk(
        &self,
        request: Request<UploadSessionFileChunkRequest>,
    ) -> Result<Response<UploadSessionFileChunkResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Reject an invalid session token before any filesystem access (parity with the other
        // session-dir methods).
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

        let host_path = crate::session_file_upload::write_upload_chunk(
            &sessions_base,
            &req.session_id,
            &req.upload_id,
            &req.file_name,
            &req.data,
            req.last,
        )?;

        Ok(Response::new(UploadSessionFileChunkResponse {
            host_path: host_path
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }))
    }

    async fn list_session_uploads(
        &self,
        request: Request<ListSessionUploadsRequest>,
    ) -> Result<Response<ListSessionUploadsResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        let sessions_base = self.uploads_sessions_base(&req.session_token)?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let uploads = crate::session_uploads::list_uploads(&sessions_base, &req.session_id)?;
        Ok(Response::new(ListSessionUploadsResponse {
            uploads: uploads
                .into_iter()
                .map(|u| SessionUploadEntry {
                    upload_id: u.upload_id,
                    file_name: u.file_name,
                    host_path: u.host_path.to_string_lossy().into_owned(),
                    size_bytes: u.size_bytes,
                    uploaded_at_ms: u.uploaded_at_ms,
                })
                .collect(),
        }))
    }

    async fn delete_session_upload(
        &self,
        request: Request<DeleteSessionUploadRequest>,
    ) -> Result<Response<DeleteSessionUploadResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        let sessions_base = self.uploads_sessions_base(&req.session_token)?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        crate::session_uploads::delete_upload(
            &sessions_base,
            &req.session_id,
            &req.upload_id,
            &req.file_name,
        )?;
        Ok(Response::new(DeleteSessionUploadResponse {}))
    }

    async fn upload_staged_attachment_chunk(
        &self,
        request: Request<UploadStagedAttachmentChunkRequest>,
    ) -> Result<Response<UploadStagedAttachmentChunkResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.resolve_os_user(&req.session_token)?;
        let local_id = local_instance_id_for_config(&self.config);

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "UploadStagedAttachmentChunk: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("UploadStagedAttachmentChunk")?;
            let inner =
                crate::livekit_peer_discovery::forward_upload_staged_attachment_chunk_via_livekit(
                    slot,
                    &peer_instance_id,
                    &req,
                )
                .await?;
            return Ok(Response::new(inner));
        }

        let staging_root =
            crate::session_attachment_staging::staging_root_for(&os_user, &self.staging_base_dir);
        let host_path = crate::session_attachment_staging::write_staged_chunk(
            &staging_root,
            &req.staging_id,
            &req.file_name,
            &req.data,
            req.last,
        )?;
        let entry = host_path.map(|path| {
            let meta = std::fs::metadata(&path).ok();
            StagedAttachmentEntry {
                daemon_instance_id: local_id,
                staging_id: req.staging_id,
                file_name: req.file_name,
                host_path: path.to_string_lossy().into_owned(),
                size_bytes: meta.as_ref().map(std::fs::Metadata::len).unwrap_or(0),
                staged_at_ms: file_mtime_ms(&path),
            }
        });
        Ok(Response::new(UploadStagedAttachmentChunkResponse { entry }))
    }

    async fn list_staged_attachments(
        &self,
        request: Request<ListStagedAttachmentsRequest>,
    ) -> Result<Response<ListStagedAttachmentsResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.resolve_os_user(&req.session_token)?;
        let local_id = local_instance_id_for_config(&self.config);

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "ListStagedAttachments: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("ListStagedAttachments")?;
            let inner = crate::livekit_peer_discovery::forward_list_staged_attachments_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(inner));
        }

        let staging_root =
            crate::session_attachment_staging::staging_root_for(&os_user, &self.staging_base_dir);
        let files = crate::session_attachment_staging::list_staged_attachments(
            &staging_root,
            &req.staging_id,
        )?;
        let attachments = files
            .into_iter()
            .map(|f| StagedAttachmentEntry {
                daemon_instance_id: local_id.clone(),
                staging_id: f.staging_id,
                file_name: f.file_name,
                host_path: f.host_path.to_string_lossy().into_owned(),
                size_bytes: f.size_bytes,
                staged_at_ms: f.staged_at_ms,
            })
            .collect();
        Ok(Response::new(ListStagedAttachmentsResponse { attachments }))
    }

    async fn delete_staged_attachment(
        &self,
        request: Request<DeleteStagedAttachmentRequest>,
    ) -> Result<Response<DeleteStagedAttachmentResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.resolve_os_user(&req.session_token)?;

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "DeleteStagedAttachment: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("DeleteStagedAttachment")?;
            let inner =
                crate::livekit_peer_discovery::forward_delete_staged_attachment_via_livekit(
                    slot,
                    &peer_instance_id,
                    &req,
                )
                .await?;
            return Ok(Response::new(inner));
        }

        let staging_root =
            crate::session_attachment_staging::staging_root_for(&os_user, &self.staging_base_dir);
        crate::session_attachment_staging::delete_staged_attachment(
            &staging_root,
            &req.staging_id,
            &req.file_name,
        )?;
        Ok(Response::new(DeleteStagedAttachmentResponse {}))
    }

    async fn read_host_document(
        &self,
        request: Request<ReadHostDocumentRequest>,
    ) -> Result<Response<ReadHostDocumentResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.resolve_os_user(&req.session_token)?;

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "ReadHostDocument: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("ReadHostDocument")?;
            let inner = crate::livekit_peer_discovery::forward_read_host_document_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(inner));
        }

        let scope =
            HostDocumentScope::try_from(req.scope).unwrap_or(HostDocumentScope::Unspecified);
        let doc = crate::host_documents::read_host_document_bytes(
            &os_user,
            &self.tddy_data_dir,
            &self.staging_base_dir,
            scope,
            &req.session_id,
            &req.project_id,
            &req.relative_path,
        )?;
        Ok(Response::new(ReadHostDocumentResponse {
            data: doc.data,
            byte_size: doc.byte_size,
        }))
    }

    /// Associated output stream type for [`stream_read_host_document`].
    type StreamReadHostDocumentStream = MpscResultStream<HostDocumentChunk>;

    /// Associated output stream type for [`stream_start_session`].
    type StreamStartSessionStream = MpscResultStream<StartSessionEvent>;

    async fn stream_read_host_document(
        &self,
        request: Request<ReadHostDocumentRequest>,
    ) -> Result<Response<Self::StreamReadHostDocumentStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.resolve_os_user(&req.session_token)?;

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "StreamReadHostDocument: forwarding stream to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("StreamReadHostDocument")?;
            // The owning host resolves the document under its own `os_user` mapping and applies
            // its own cap, so nothing is read locally here. A peer-side failure — or a stream that
            // stops without its terminator — arrives as an error item, terminating this stream.
            let rx = crate::livekit_peer_discovery::forward_stream_read_host_document_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(MpscResultStream { rx }));
        }

        let scope =
            HostDocumentScope::try_from(req.scope).unwrap_or(HostDocumentScope::Unspecified);
        let resolved = crate::host_documents::resolve_host_document(
            &os_user,
            &self.tddy_data_dir,
            &self.staging_base_dir,
            scope,
            &req.session_id,
            &req.project_id,
            &req.relative_path,
        )?;

        // The cap is checked before the first frame, so an over-cap document is refused rather
        // than streamed and cut short: a consumer cannot tell a truncated document from a whole
        // one once the frames have started.
        let max_bytes = self.config.max_attachment_bytes;
        if resolved.byte_size > max_bytes {
            return Err(Status::invalid_argument(format!(
                "host document exceeds this host's maximum attachment size of {max_bytes} bytes"
            )));
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<HostDocumentChunk, Status>>();
        tokio::task::spawn_blocking(move || {
            stream_document_frames(&resolved.path, resolved.byte_size, &tx);
        });
        Ok(Response::new(MpscResultStream { rx }))
    }

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
            let rx = crate::livekit_peer_discovery::forward_stream_start_session_via_livekit(
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
