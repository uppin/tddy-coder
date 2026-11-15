//! `UpdateProject`: rename a project on every host owning it.

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{UpdateProjectRequest, UpdateProjectResponse};

use super::ProjectRpcHandler;

impl ProjectRpcHandler {
    pub(crate) async fn update_project_at_project_coordinate(
        &self,
        _request: Request<UpdateProjectRequest>,
    ) -> Result<Response<UpdateProjectResponse>, Status> {
        Err(Status::unimplemented("red: UpdateProject"))
    }
}
