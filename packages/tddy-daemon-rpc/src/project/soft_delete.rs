//! `DeleteProject` and `RestoreProject`: soft-delete a project, and bring it back, on every host
//! owning it.

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{
    DeleteProjectRequest, DeleteProjectResponse, RestoreProjectRequest, RestoreProjectResponse,
};

use super::ProjectRpcHandler;

impl ProjectRpcHandler {
    pub(crate) async fn delete_project_at_project_coordinate(
        &self,
        _request: Request<DeleteProjectRequest>,
    ) -> Result<Response<DeleteProjectResponse>, Status> {
        Err(Status::unimplemented("red: DeleteProject"))
    }

    pub(crate) async fn restore_project_at_project_coordinate(
        &self,
        _request: Request<RestoreProjectRequest>,
    ) -> Result<Response<RestoreProjectResponse>, Status> {
        Err(Status::unimplemented("red: RestoreProject"))
    }
}
