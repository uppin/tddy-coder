//! Family C session RPCs at the `session.SessionService` coordinate.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use uuid::Uuid;

use super::{service_util, AttachmentProgressSink, DaemonSessionHost, MpscResultStream};
use crate::connection_service::hooks_and_urls;
use crate::{session_deletion, session_list_enrichment, session_reader};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::{read_session_metadata, Changeset};
use tddy_rpc::{Request, Response, Status};
use tddy_spawn::{spawn_worker, spawner};
use tddy_spawn::spawner::SpawnOptions;
use tddy_service::proto::session::SessionEntry as ConnSessionEntry;
use tddy_service::proto::session::StartSessionEvent as ConnStartSessionEvent;
use tddy_service::proto::session::start_session_event::Event as StartSessionEventKind;
use tddy_service::proto::session::{
    ConnectSessionRequest, ConnectSessionResponse, DeleteSessionRequest, DeleteSessionResponse,
    GetWorktreeSnapshotRequest, GetWorktreeSnapshotResponse, ListSessionsRequest,
    ListSessionsResponse, ResumeSessionRequest, ResumeSessionResponse, SessionEntry,
    SignalSessionRequest, SignalSessionResponse, StartSessionEvent, StartSessionRequest,
    StartSessionResponse,
};
use tddy_service::proto::session::Signal;
use crate::livekit_peer_discovery::{local_instance_id_for_config, PeerRoute};
use crate::user_sessions_path::projects_path_for_user;
use tddy_service::proto::exec_tools::ExecuteToolRequest;
use tddy_service::proto::session::ResumeSessionResponse as ConnResumeSessionResponse;

/// The coordinate a forwarded call from the legacy connection service is addressed at on the peer.
const SESSION_SERVICE: &str = "session.SessionService";

fn bridge_conn_resume_response(
    resp: Result<Response<ConnResumeSessionResponse>, Status>,
) -> Result<Response<ResumeSessionResponse>, Status> {
    let resp = resp?;
    Ok(Response::new(
        super::family_proto_bridge::wire_same(&resp.into_inner())?,
    ))
}

impl DaemonSessionHost {

pub(crate) async fn list_sessions_at_session_coordinate(
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
                    let mut entry = SessionEntry {
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
                        agent_status: tddy_service::proto::types::SessionAgentStatus::Unspecified
                            as i32,
                        last_activity: None,
                    };
                    let mut conn_entry: ConnSessionEntry = super::family_proto_bridge::wire_same(&entry)
                        .map_err(|s| anyhow::anyhow!(s.to_string()))?;
                    if let Err(e) = session_list_enrichment::apply_session_list_status_to_proto(
                        &session_dir,
                        &mut conn_entry,
                    ) {
                        log::warn!(
                            target: "tddy_daemon::connection_service",
                            "ListSessions: enrichment failed for {}: {}",
                            session_dir.display(),
                            e
                        );
                    }
                    entry = super::family_proto_bridge::wire_same(&conn_entry)
                        .map_err(|s| anyhow::anyhow!(s.to_string()))?;
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

    pub(crate) async fn start_session_at_session_coordinate(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let conn_req: tddy_service::proto::session::StartSessionRequest =
            super::family_proto_bridge::wire_same(&request.into_inner())?;
        let conn_resp = self
            .start_session_core(conn_req, &AttachmentProgressSink::discarding())
            .await?;
        Ok(Response::new(
            super::family_proto_bridge::wire_same(&conn_resp.into_inner())?,
        ))
    }

pub(crate) async fn connect_session_at_session_coordinate(
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

pub(crate) async fn resume_session_at_session_coordinate(
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
            return bridge_conn_resume_response(
                self.resume_claude_cli_session(
                    os_user,
                    &req.session_id,
                    session_dir,
                    metadata,
                    &req.session_token,
                )
                .await,
            );
        }

        // --- cursor-cli branch: resume without LiveKit ---
        if metadata.session_type.as_deref() == Some("cursor-cli") {
            return bridge_conn_resume_response(
                crate::cursor_cli_spawn::resume_cursor_cli_session(
                    &self.claude_cli_manager,
                    &self.config,
                    &req.session_id,
                    &session_dir,
                    metadata,
                )
                .await,
            );
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

pub(crate) async fn signal_session_at_session_coordinate(
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
            use tddy_service::proto::session::Signal;

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

pub(crate) async fn delete_session_at_session_coordinate(
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

pub(crate) async fn get_worktree_snapshot_at_session_coordinate(
        &self,
        request: Request<GetWorktreeSnapshotRequest>,
    ) -> Result<Response<GetWorktreeSnapshotResponse>, Status> {
        let req = request.into_inner();

        // Routed exactly like ExecuteTool, and for the same reason: the caller names the daemon it
        // believes holds the checkout, and a session room on the agent's daemon polls a remote
        // checkout by addressing the codebase daemon. Sharing the classifier and the forward keeps
        // one answer to "which daemon owns this session's files".
        if let Some(answered) = self
            .rpc_served_by_peer(
                SESSION_SERVICE,
                "GetWorktreeSnapshot",
                &req.daemon_instance_id,
                &req,
            )
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

    /// Local peer-trust minting is not available on this transport. Peer credentials
    /// (SO_PEERCRED) exist only on the daemon's local Unix-domain socket; over ConnectRPC-HTTP or
    /// LiveKit there is no peer uid to trust, so those transports reach this tddy-rpc handler and
    /// are rejected. The UDS tonic adapter handles `MintLocalToken` itself and never delegates here.

    pub(crate) async fn stream_start_session_at_session_coordinate(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<MpscResultStream<StartSessionEvent>>, Status> {
        self.record_rpc_activity();
        let req: tddy_service::proto::session::StartSessionRequest = super::family_proto_bridge::wire_same(&request.into_inner())?;
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
            let (tx, session_rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(async move {
                let mut conn_rx = rx;
                while let Some(item) = conn_rx.recv().await {
                    let mapped = match item {
                        Ok(ev) => super::family_proto_bridge::wire_same(&ev),
                        Err(s) => Err(s),
                    };
                    if tx.send(mapped).is_err() {
                        break;
                    }
                }
            });
            return Ok(Response::new(MpscResultStream { rx: session_rx }));
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<StartSessionEvent, Status>>();
        // The work runs on its own task so progress reaches the client while the host is still
        // materializing, rather than all at once after the start completes. A failure terminates
        // the stream with the status — a result event is only ever sent on success.
        let service = self.clone();
        let (conn_progress_tx, conn_progress_rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<ConnStartSessionEvent, Status>>();
        let session_tx = tx.clone();
        tokio::spawn(async move {
            let mut conn_progress_rx = conn_progress_rx;
            while let Some(item) = conn_progress_rx.recv().await {
                let mapped = match item {
                    Ok(ev) => super::family_proto_bridge::wire_same(&ev),
                    Err(s) => Err(s),
                };
                if session_tx.send(mapped).is_err() {
                    break;
                }
            }
        });
        tokio::spawn(async move {
            let sink = AttachmentProgressSink::streaming(conn_progress_tx);
            let event = match service.start_session_core(req, &sink).await {
                Ok(response) => match super::family_proto_bridge::wire_same(&response.into_inner()) {
                    Ok(session_result) => Ok(StartSessionEvent {
                        event: Some(StartSessionEventKind::Result(session_result)),
                    }),
                    Err(status) => Err(status),
                },
                Err(status) => Err(status),
            };
            let _ = tx.send(event);
        });
        Ok(Response::new(MpscResultStream { rx }))
    }
}
