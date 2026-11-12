use tddy_session_split::service_util;

use crate::svc_start_claude_cli_session::ToolSessionHostRegistration;

use crate::svc_start_session_core::ToolSpawnPlan;
use crate::svc_start_session_core::ToolSpawnPurpose;

use std::path::PathBuf;

use crate::session_coordinate_handlers::bridge_conn_resume_response;

use tddy_core::read_session_metadata;

use crate::harness_from_spec::{recorded_session_launch, SessionLaunch};

use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_core::session_lifecycle::validate_session_id_segment;

use tddy_rpc::Status;

use tddy_service::proto::session::ResumeSessionResponse;

use tddy_rpc::Response;

use tddy_service::proto::session::ResumeSessionRequest;

use crate::launch_ports::LaunchSessions;
use crate::tool_session_route::resumed_tool_session_route_json;
use tddy_rpc::Request;
use tddy_spawn::spawner;

impl LaunchSessions {
    pub async fn resume_session_at_session_coordinate(
        &self,
        request: Request<ResumeSessionRequest>,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = &self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;
        self.fast_forward_a_resumed_orchestrators_home(os_user, &metadata)?;

        // How the session resumes is read from the session type it records, as this host declares
        // it: every CLI agent through the one spec-driven launch, a no-agent session by re-provisioning
        // its jail, and a `tddy_coder` session by respawning its recorded tool.
        let launch = recorded_session_launch(
            &self.coding_agents,
            metadata.session_type.as_deref(),
            metadata.tool.as_deref(),
        )?;
        if let SessionLaunch::AgentCli(kind) = launch {
            // A sandboxed session resumes in its jail and never on the host's PTY.
            if metadata.sandbox == Some(true) {
                return bridge_conn_resume_response(
                    self.resume_sandboxed_cli_harness(kind, os_user, &req, &session_dir, metadata)
                        .await,
                );
            }
            return bridge_conn_resume_response(
                self.resume_cli_harness(
                    kind,
                    os_user,
                    &req,
                    &sessions_base,
                    &session_dir,
                    metadata,
                )
                .await,
            );
        }

        // --- no-agent branch: re-provision jail when sandboxed, no LiveKit ---
        if launch == SessionLaunch::NoAgent {
            if metadata.sandbox == Some(true)
                && self
                    .workspace_sandboxes
                    .get(&req.session_id)
                    .await
                    .is_none()
            {
                self.split_sessions
                    .provision_workspace_tool_sandbox(&sessions_base, &req.session_id)
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
        let os_user = os_user.to_string();
        // The spawn closure below takes ownership; the presenter observer started afterwards needs
        // the same user to resolve the session's label from its sessions directory.
        let observer_os_user = os_user.clone();
        let (resume_agent, resume_recipe) = service_util::resume_agent_and_recipe(&metadata);
        // A resumed session's agent is resolved the same way a starting one's is, so an assistant
        // this daemon defined still reaches the child as a def it can build a backend from.
        let resume_agent_def: Option<String> = match resume_agent.as_deref() {
            Some(name) => self
                .agent_roster
                .agent_def_for_spawn(name, &github_user)
                .await?
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| Status::internal(format!("failed to serialize agent def: {e}")))?,
            None => None,
        };
        // A session started on a provider account resumes on it: the account its start recorded is
        // re-resolved against the owner's vault as it stands now, and a refusal resumes nothing.
        let provider_route_json = resumed_tool_session_route_json(
            &self.host.session_account_access(&req.session_token),
            Some(&*self.coding_agents),
            resume_agent.as_deref().unwrap_or_default(),
            &session_dir,
        )?;
        // One resolution for the project this session belongs to, as on start; a project that no
        // longer resolves resumes the session under the checkout's own identity, with the reason
        // logged. Only the commit pairs go to the child — never the account's token.
        let identity = self.host.session_identity(
            &os_user,
            &req.session_id,
            &metadata.project_id,
            &req.session_token,
        );
        let git_environment = identity.git_environment;
        // The session registers again on its user's host-session socket — the same socket it used
        // before — so it answers from the refreshed assignments and the token this resume carries.
        let host_session_socket = self
            .register_tool_session_on_host_socket(ToolSessionHostRegistration {
                os_user: &os_user,
                session_id: &req.session_id,
                project_id: &metadata.project_id,
                recipe: resume_recipe.as_deref(),
                model: None,
                account_access: self.host.session_account_access(&req.session_token),
                github_credential: identity.github_credential,
            })
            .await;
        let spawned = self
            .spawn_tddy_coder(ToolSpawnPlan {
                purpose: ToolSpawnPurpose::Resume,
                os_user,
                tool_path,
                repo_path,
                livekit,
                resume_session_id: Some(req.session_id.clone()),
                new_session_id: None,
                project_id: Some(metadata.project_id.clone()).filter(|id| !id.is_empty()),
                agent: resume_agent,
                agent_def_json: resume_agent_def,
                provider_route_json,
                recipe: resume_recipe,
                stack_parent: None,
                stack_node_id: None,
                // Seeding a stack is a creation-time act; a resumed orchestrator already has
                // whatever stack it was created with.
                stack_seed_base_session: None,
                model: None,
                host_session_socket,
                git_environment,
            })
            .await;
        self.tool_session_spawned(&req.session_id, &spawned);
        let result = spawned?;
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
}
