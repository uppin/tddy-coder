//! Family A — `catalog.CatalogService`.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_discovery::CatalogHandler;
use tddy_model_registry::ModelRegistryStore;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::catalog::{
    ListAgentModelsRequest, ListAgentModelsResponse, ListAgentsRequest, ListAgentsResponse,
    ListSubagentsRequest, ListSubagentsResponse, ListToolsRequest, ListToolsResponse,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_task::IdleTimeoutTracker;

/// The four catalogue RPCs: the configured tools and agents, the model registry's assistants, and
/// the idle tracker every call bumps.
// TODO(#carve 11): the fields are read by the moved bodies; drop this allow when they land.
#[allow(dead_code)]
pub struct CatalogRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    model_registry: Option<Arc<ModelRegistryStore>>,
    idle_tracker: Option<Arc<IdleTimeoutTracker>>,
}

impl CatalogRpcHandler {
    /// A handler sharing `host`'s state — the same model registry and the same idle tracker.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        // TODO(#carve 11): clone the host's catalogue fields once the host exposes them.
        let _ = host;
        todo!("CatalogRpcHandler::from_host")
    }
}

#[async_trait]
impl CatalogHandler for CatalogRpcHandler {
    async fn list_tools(
        &self,
        _request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        todo!("CatalogRpcHandler::list_tools")
    }

    async fn list_agents(
        &self,
        _request: Request<ListAgentsRequest>,
    ) -> Result<Response<ListAgentsResponse>, Status> {
        todo!("CatalogRpcHandler::list_agents")
    }

    async fn list_agent_models(
        &self,
        _request: Request<ListAgentModelsRequest>,
    ) -> Result<Response<ListAgentModelsResponse>, Status> {
        todo!("CatalogRpcHandler::list_agent_models")
    }

    async fn list_subagents(
        &self,
        _request: Request<ListSubagentsRequest>,
    ) -> Result<Response<ListSubagentsResponse>, Status> {
        todo!("CatalogRpcHandler::list_subagents")
    }
}
