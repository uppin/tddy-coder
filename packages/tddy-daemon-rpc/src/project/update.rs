//! `UpdateProject`: edit a project on every host owning it ([`super::fan_out`]). The display name
//! and the repository identity (`git_url`) belong to the logical project and fan out; the main
//! checkout path is each host's own, so it is applied only by the host the change is addressed to
//! (`local_only`). `project_id` never changes, and the default branch and accounts keep their own
//! RPCs.

use tddy_projects::project_storage;
use tddy_rpc::{Code, Request, Response, Status};
use tddy_service::proto::project::{UpdateProjectRequest, UpdateProjectResponse};

use super::soft_delete::{not_on_any_reachable_host, required_project_id, ProjectCaller};
use super::ProjectRpcHandler;

impl ProjectRpcHandler {
    pub(crate) async fn update_project_at_project_coordinate(
        &self,
        request: Request<UpdateProjectRequest>,
    ) -> Result<Response<UpdateProjectResponse>, Status> {
        let req = request.into_inner();
        let caller = self.project_caller(&req.session_token)?;
        let project_id = required_project_id(&req.project_id)?;
        // Refused before any host changes.
        let name = project_storage::validate_project_name(&req.name)
            .map_err(Status::invalid_argument)?
            .to_string();
        // Empty = leave unchanged.
        let git_url = req.git_url.trim();
        let git_url = (!git_url.is_empty()).then(|| git_url.to_string());
        let main_repo_path = req.main_repo_path.trim();
        if !main_repo_path.is_empty() && !req.local_only {
            // Each host owns its own checkout: a path edit names one host's filesystem, so fanning
            // it out would move every host's row to this host's path.
            return Err(Status::invalid_argument(
                "main_repo_path is per host and is applied only with local_only",
            ));
        }
        let main_repo_path = (!main_repo_path.is_empty()).then(|| main_repo_path.to_string());
        let here = match self.apply_here(
            &caller,
            &project_id,
            &name,
            git_url.as_deref(),
            main_repo_path.as_deref(),
        ) {
            Ok(answer) => Some(answer),
            Err(status) if status.code == Code::NotFound => None,
            Err(status) => return Err(status),
        };
        if req.local_only {
            return here
                .map(Response::new)
                .ok_or_else(|| Status::not_found("project not found"));
        }
        // The path edit never fans out: it was applied here alone (or refused above).
        let forwarded = UpdateProjectRequest {
            name,
            git_url: git_url.unwrap_or_default(),
            main_repo_path: String::new(),
            local_only: true,
            ..req
        };
        let peers = self
            .fan_out_to_peers(
                "UpdateProject",
                forwarded.clone(),
                |room, peer, r| async move {
                    tddy_daemon_kernel::peer_calls::forward_update_project(&room, &peer, &r).await
                },
            )
            .await;
        let unconfirmed_hosts = peers.unconfirmed();
        let answers = here
            .into_iter()
            .chain(peers.applied.into_iter().map(|(_, a)| a))
            .collect::<Vec<UpdateProjectResponse>>();
        let Some(first) = answers.first() else {
            return Err(not_on_any_reachable_host(&project_id));
        };
        log::info!(
            "UpdateProject: project_id={project_id} applied_hosts={}",
            answers.len()
        );
        Ok(Response::new(UpdateProjectResponse {
            project: first.project.clone(),
            applied_hosts: answers
                .iter()
                .flat_map(|a| a.applied_hosts.clone())
                .collect(),
            unconfirmed_hosts,
        }))
    }

    fn apply_here(
        &self,
        caller: &ProjectCaller,
        project_id: &str,
        name: &str,
        git_url: Option<&str>,
        main_repo_path: Option<&str>,
    ) -> Result<UpdateProjectResponse, Status> {
        if let Some(path) = main_repo_path {
            // The host applying the change owns the check: a path that is not a directory on this
            // host is a typo, and storing it would break every worktree setup that reads the row.
            if !std::path::Path::new(path).is_dir() {
                return Err(Status::invalid_argument(
                    "the project's main checkout path does not exist on this host",
                ));
            }
        }
        let row = project_storage::update_project(
            &caller.projects_dir,
            project_id,
            Some(name),
            git_url,
            main_repo_path,
        )
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("project not found"))?;
        Ok(UpdateProjectResponse {
            project: Some(self.local_entry(caller, &row)),
            applied_hosts: vec![caller.local_id.clone()],
            unconfirmed_hosts: Vec::new(),
        })
    }
}
