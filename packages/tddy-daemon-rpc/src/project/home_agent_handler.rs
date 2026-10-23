//! `SetProjectHomeAgent` at the project coordinate: route like the default branch, check the id
//! against the agent catalog, then write `projects.yaml`.
//!
//! Its own module because `coordinate_handlers.rs` is over the file-size budget
//! (`packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`).

use std::path::PathBuf;

use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_projects::project_storage;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{SetProjectHomeAgentRequest, SetProjectHomeAgentResponse};
use tddy_session_lifecycle::connection_service::wire_same;

use super::coordinate_route::CoordinateRoute;
use super::entries;
use super::ProjectRpcHandler;
use crate::catalog::listed_agents;

impl ProjectRpcHandler {
    /// Set or clear `project_id`'s home agent on this host or the peer named by
    /// `daemon_instance_id`. An empty id clears it. Any other id must be one this daemon's agent
    /// catalog lists; an unlisted id is `invalid_argument`, and nothing is written.
    pub(crate) async fn set_project_home_agent_at_project_coordinate(
        &self,
        request: Request<SetProjectHomeAgentRequest>,
    ) -> Result<Response<SetProjectHomeAgentResponse>, Status> {
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

        // Route to the requested host: local (empty / matching id) or forward to a peer daemon. The
        // catalog check runs on the daemon that writes, so a forwarded call is checked against the
        // peer's own agents.
        let local_id =
            match self.coordinate_route("SetProjectHomeAgent", &req.daemon_instance_id)? {
                CoordinateRoute::Local { local_id } => local_id,
                CoordinateRoute::Forward {
                    room,
                    peer_instance_id,
                } => {
                    let inner = tddy_daemon_kernel::peer_calls::forward_set_project_home_agent(
                        room,
                        &peer_instance_id,
                        &req,
                    )
                    .await?;
                    return Ok(Response::new(inner));
                }
            };

        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;

        // Check the id before the registry is touched.
        let stored_agent = self.checked_home_agent(req.home_agent.trim()).await?;

        if project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .is_none()
        {
            return Err(Status::not_found("project not found"));
        }

        project_storage::set_project_home_agent(&projects_dir, project_id, stored_agent)
            .map_err(|e| Status::internal(e.to_string()))?;

        let stored = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::internal("project vanished after write"))?;
        log::info!(
            "SetProjectHomeAgent: project_id={} home_agent={}",
            project_id,
            stored.home_agent.as_deref().unwrap_or_default()
        );
        let repo_root = PathBuf::from(&stored.main_repo_path);
        let default_remote =
            entries::resolve_default_remote_or_empty(&projects_dir, &stored.project_id, &repo_root);
        Ok(Response::new(SetProjectHomeAgentResponse {
            project: Some(wire_same(&entries::project_entry_from(
                &stored,
                local_id,
                default_remote,
            ))?),
        }))
    }

    /// The value to store for `home_agent`: `None` for an empty id (clearing needs no catalog
    /// check), the id itself when this daemon's agent catalog lists it, `invalid_argument` otherwise.
    async fn checked_home_agent<'a>(&self, home_agent: &'a str) -> Result<Option<&'a str>, Status> {
        if home_agent.is_empty() {
            return Ok(None);
        }
        let listed = listed_agents(
            &self.config,
            &self.tddy_data_dir,
            self.model_registry.as_deref(),
        )
        .await?;
        if listed.iter().any(|agent| agent.id == home_agent) {
            return Ok(Some(home_agent));
        }
        log::info!(
            "SetProjectHomeAgent: refused home_agent={} not listed by this daemon",
            home_agent
        );
        Err(Status::invalid_argument(format!(
            "home agent {home_agent} is not listed by this daemon's agent catalog"
        )))
    }
}
