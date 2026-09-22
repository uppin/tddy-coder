//! Family D — `project.ProjectService`.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use livekit::prelude::Room;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_host_service::multi_host::EligibleDaemonSource;
use tddy_projects::ProjectHandler;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::project::{
    AddProjectToHostRequest, AddProjectToHostResponse, CreateProjectRequest, CreateProjectResponse,
    ListProjectBranchesRequest, ListProjectBranchesResponse, ListProjectsRequest,
    ListProjectsResponse, SetProjectDefaultBranchRequest, SetProjectDefaultBranchResponse,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_spawn::spawn_worker::SpawnClient;

/// The five project RPCs, holding only what they read: the caller's identity, the projects store
/// under `tddy_data_dir`, the peers a listing fans out to, and the spawn client a clone runs through.
// TODO(#carve 11): the fields are read by the moved bodies; drop this allow when they land.
#[allow(dead_code)]
pub struct ProjectRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    spawn_client: Option<Arc<SpawnClient>>,
    common_room_livekit_room: Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>>,
}

impl ProjectRpcHandler {
    /// A handler sharing `host`'s state — the same peer source, spawn client and room slot.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        Self {
            config: host.config().clone(),
            user_resolver: host.user_resolver(),
            tddy_data_dir: host.tddy_data_dir().to_path_buf(),
            eligible_daemon_source: host.eligible_daemon_source(),
            spawn_client: host.spawn_client(),
            common_room_livekit_room: host.common_room_livekit_room(),
        }
    }
}

#[async_trait]
impl ProjectHandler for ProjectRpcHandler {
    async fn list_projects(
        &self,
        _request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        todo!("ProjectRpcHandler::list_projects")
    }

    async fn create_project(
        &self,
        _request: Request<CreateProjectRequest>,
    ) -> Result<Response<CreateProjectResponse>, Status> {
        todo!("ProjectRpcHandler::create_project")
    }

    async fn add_project_to_host(
        &self,
        _request: Request<AddProjectToHostRequest>,
    ) -> Result<Response<AddProjectToHostResponse>, Status> {
        todo!("ProjectRpcHandler::add_project_to_host")
    }

    async fn list_project_branches(
        &self,
        _request: Request<ListProjectBranchesRequest>,
    ) -> Result<Response<ListProjectBranchesResponse>, Status> {
        todo!("ProjectRpcHandler::list_project_branches")
    }

    async fn set_project_default_branch(
        &self,
        _request: Request<SetProjectDefaultBranchRequest>,
    ) -> Result<Response<SetProjectDefaultBranchResponse>, Status> {
        todo!("ProjectRpcHandler::set_project_default_branch")
    }
}
