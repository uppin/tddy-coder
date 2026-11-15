use crate::project::clone_destination::project_path_under_home_from_user_relative;
use crate::project::coordinate_route::CoordinateRoute;
use crate::project::entries::{self};
use crate::project::ProjectRpcHandler;
use std::path::PathBuf;
use tddy_daemon_kernel::user_paths::{projects_path_for_user, repos_base_for_user};
use tddy_projects::project_storage::{self, ProjectData};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{AddProjectToHostRequest, AddProjectToHostResponse};
use tddy_session_lifecycle::connection_service::wire_same;

impl ProjectRpcHandler {
    pub(crate) async fn add_project_to_host_at_project_coordinate(
        &self,
        request: Request<AddProjectToHostRequest>,
    ) -> Result<Response<AddProjectToHostResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = &self
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
        let local_id = match self.coordinate_route("AddProjectToHost", &req.daemon_instance_id)? {
            CoordinateRoute::Local { local_id } => local_id,
            CoordinateRoute::Forward {
                room,
                peer_instance_id,
            } => {
                let inner = tddy_daemon_kernel::peer_calls::forward_add_project_to_host(
                    room,
                    &peer_instance_id,
                    &wire_same(&req)?,
                )
                .await?;
                return Ok(Response::new(wire_same(&inner)?));
            }
        };

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
            let default_remote = entries::resolve_default_remote_or_empty(
                &projects_dir,
                &existing.project_id,
                &repo_root,
            );
            return Ok(Response::new(AddProjectToHostResponse {
                project: Some(wire_same(&entries::project_entry_from(
                    &existing,
                    local_id,
                    default_remote,
                ))?),
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
        self.clone_without_deadline("add_project_to_host", os_user, git_url, &destination)
            .await?;

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
            accounts: Vec::new(),
            local_gh: false,
            home_session_id: None,
            orchestrator_defaults: None,
            lifecycle: Default::default(),
        };
        let (stored, _created) = project_storage::add_or_get_project(&projects_dir, project)
            .map_err(|e| Status::internal(e.to_string()))?;

        let repo_root = PathBuf::from(&stored.main_repo_path);
        let default_remote =
            entries::resolve_default_remote_or_empty(&projects_dir, &stored.project_id, &repo_root);
        Ok(Response::new(AddProjectToHostResponse {
            project: Some(wire_same(&entries::project_entry_from(
                &stored,
                local_id,
                default_remote,
            ))?),
        }))
    }
}
