//! Family D project RPCs at the `project.ProjectService` coordinate.

use std::path::PathBuf;

use uuid::Uuid;

use super::hooks_and_urls;
use super::{merge_listed_projects_with_peers, service_util, DaemonSessionHost};
use tddy_spawn::{spawn_worker, spawner};
use crate::livekit_peer_discovery::{local_instance_id_for_config, PeerRoute};
use crate::project_storage::{self, ProjectData};
use crate::user_sessions_path::{
    project_path_under_home_from_user_relative, projects_path_for_user, repos_base_for_user,
};
use super::family_proto_bridge::wire_same;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::ProjectEntry as ConnProjectEntry;
use tddy_service::proto::project::{
    AddProjectToHostRequest, AddProjectToHostResponse, CreateProjectRequest, CreateProjectResponse,
    ListProjectBranchesRequest, ListProjectBranchesResponse, ListProjectsRequest,
    ListProjectsResponse, ProjectEntry, SetProjectDefaultBranchRequest,
    SetProjectDefaultBranchResponse,
};

impl DaemonSessionHost {

pub(crate) async fn list_projects_at_project_coordinate(
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
        let entries: Vec<ConnProjectEntry> = projects
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
        let conn_projects = if req.local_only {
            entries
        } else {
            merge_listed_projects_with_peers(
                &*self.eligible_daemon_source,
                &req.session_token,
                entries,
            )
            .await
        };
        let projects = conn_projects
            .iter()
            .map(wire_same)
            .collect::<Result<Vec<ProjectEntry>, Status>>()?;
        Ok(Response::new(ListProjectsResponse { projects }))
    }

pub(crate) async fn create_project_at_project_coordinate(
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
            project: Some(wire_same(&entry)?),
        }))
    }

pub(crate) async fn add_project_to_host_at_project_coordinate(
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
            let conn_req: tddy_service::proto::project::AddProjectToHostRequest =
                wire_same(&req)?;
            let inner = tddy_daemon_livekit::livekit_peer_discovery::forward_add_project_to_host_via_livekit(
                slot,
                &peer_instance_id,
                &conn_req,
            )
            .await?;
            return Ok(Response::new(wire_same(&inner)?));
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
                project: Some(wire_same(
                    &hooks_and_urls::project_entry_from(&existing, local_id, default_remote),
                )?),
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
            project: Some(wire_same(
                &hooks_and_urls::project_entry_from(&stored, local_id, default_remote),
            )?),
        }))
    }

pub(crate) async fn set_project_default_branch_at_project_coordinate(
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
            let conn_req: tddy_service::proto::project::SetProjectDefaultBranchRequest =
                wire_same(&req)?;
            let inner =
                tddy_daemon_livekit::livekit_peer_discovery::forward_set_project_default_branch_via_livekit(
                    slot,
                    &peer_instance_id,
                    &conn_req,
                )
                .await?;
            return Ok(Response::new(wire_same(&inner)?));
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
            project: Some(wire_same(
                &hooks_and_urls::project_entry_from(&stored, local_id, default_remote),
            )?),
        }))
    }

pub(crate) async fn list_project_branches_at_project_coordinate(
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

}
