//! `UpdateProject`: rename a project on every host owning it ([`super::fan_out`]). Only the display
//! name is editable here: the repository identity (`git_url`, `main_repo_path`) and `project_id`
//! never change, and the default branch and accounts keep their own RPCs.

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
        let here = match self.rename_here(&caller, &project_id, &name) {
            Ok(answer) => Some(answer),
            Err(status) if status.code == Code::NotFound => None,
            Err(status) => return Err(status),
        };
        if req.local_only {
            return here
                .map(Response::new)
                .ok_or_else(|| Status::not_found("project not found"));
        }
        let forwarded = UpdateProjectRequest {
            name,
            local_only: true,
            ..req
        };
        let peers = self
            .fan_out_to_peers("UpdateProject", forwarded.clone(), |room, peer, r| async move {
                tddy_daemon_kernel::peer_calls::forward_update_project(&room, &peer, &r).await
            })
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
            applied_hosts: answers.iter().flat_map(|a| a.applied_hosts.clone()).collect(),
            unconfirmed_hosts,
        }))
    }

    fn rename_here(
        &self,
        caller: &ProjectCaller,
        project_id: &str,
        name: &str,
    ) -> Result<UpdateProjectResponse, Status> {
        let row = project_storage::rename_project(&caller.projects_dir, project_id, name)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        Ok(UpdateProjectResponse {
            project: Some(self.local_entry(caller, &row)),
            applied_hosts: vec![caller.local_id.clone()],
            unconfirmed_hosts: Vec::new(),
        })
    }
}
