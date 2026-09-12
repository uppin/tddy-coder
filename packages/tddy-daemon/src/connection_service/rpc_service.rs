// The parent module's own imports, carried in: this block was the whole `impl
// ConnectionServiceTrait` and reaches the same traits and helpers it always did. Unused
// entries are pruned below by the compiler's own spans.
// `encode_to_vec` is a `prost::Message` method; the trait is imported anonymously because
// only its methods are used.
use tddy_service::proto::connection::start_session_event::Event as StartSessionEventKind;
use tddy_service::proto::connection::ConnectionService as ConnectionServiceTrait;
use tddy_service::proto::connection::SessionEntry as ProtoSessionEntry;
use tddy_service::proto::connection::{
    DemoVmState, GetDemoVmStatusRequest, GetDemoVmStatusResponse, StartDemoVmRequest,
    StartDemoVmResponse, StopDemoVmRequest, StopDemoVmResponse,
};
use tddy_service::proto::connection::{ExecuteToolRequest, ProjectEntry as ProtoProjectEntry};
use tddy_service::proto::connection::{
    GetWorktreeSnapshotRequest, GetWorktreeSnapshotResponse, MintLocalTokenRequest,
    MintLocalTokenResponse, StartSessionEvent,
};

use crate::{
    connection_service::{activity_hub, hooks_and_urls, service_util},
    project_storage, session_deletion, session_list_enrichment, session_reader,
};
use tddy_spawn::{spawn_worker, spawner};

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

use std::sync::Arc;

use uuid::Uuid;

use super::MpscResultStream;

use crate::livekit_peer_discovery::local_instance_id_for_config;

use std::path::PathBuf;

use std::path::Path;

use tddy_rpc::Status;

use tddy_rpc::Response;

use tddy_rpc::Request;

use super::ConnectionServiceImpl;

/// The coordinate a forwarded call from *this* service is addressed at on the peer. A forward has
/// to land on the same method of the same service there, and the four routed unaries below are the
/// ones this service still declares.
const CONNECTION_SERVICE: &str = "connection.ConnectionService";

#[async_trait::async_trait]
impl ConnectionServiceTrait for ConnectionServiceImpl {
    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.list_sessions_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
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
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.start_session_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn connect_session(
        &self,
        request: Request<ConnectSessionRequest>,
    ) -> Result<Response<ConnectSessionResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.connect_session_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn resume_session(
        &self,
        request: Request<ResumeSessionRequest>,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.resume_session_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn signal_session(
        &self,
        request: Request<SignalSessionRequest>,
    ) -> Result<Response<SignalSessionResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.signal_session_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.delete_session_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
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

    async fn get_worktree_snapshot(
        &self,
        request: Request<GetWorktreeSnapshotRequest>,
    ) -> Result<Response<GetWorktreeSnapshotResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.get_worktree_snapshot_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
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
