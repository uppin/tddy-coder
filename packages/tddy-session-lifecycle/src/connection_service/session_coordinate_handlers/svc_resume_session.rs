use crate::connection_service::service_util;

use super::DaemonSessionHost;

use std::path::PathBuf;

use super::bridge_conn_resume_response;

use tddy_core::read_session_metadata;

use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_core::session_lifecycle::validate_session_id_segment;

use tddy_rpc::Status;

use tddy_service::proto::session::ResumeSessionResponse;

use tddy_rpc::Response;

use tddy_service::proto::session::ResumeSessionRequest;

use tddy_rpc::Request;
use tddy_spawn::{spawn_worker, spawner::{self, SpawnOptions}};

impl DaemonSessionHost {
    pub(crate) async fn resume_session_at_session_coordinate(
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
                        &sessions_base,
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
}
