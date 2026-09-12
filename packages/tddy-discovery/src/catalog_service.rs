//! `catalog.CatalogService` — family A, served from this crate.
//!
//! Handlers delegate to a host the daemon wires; see [`CatalogHandler`].

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::catalog::{
    CatalogService, ListAgentModelsRequest, ListAgentModelsResponse, ListAgentsRequest,
    ListAgentsResponse, ListSubagentsRequest, ListSubagentsResponse, ListToolsRequest,
    ListToolsResponse,
};

/// What the four catalogue RPCs need from the host process.
#[async_trait]
pub trait CatalogHandler: Send + Sync {
    async fn list_tools(
        &self,
        request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status>;

    async fn list_agents(
        &self,
        request: Request<ListAgentsRequest>,
    ) -> Result<Response<ListAgentsResponse>, Status>;

    async fn list_agent_models(
        &self,
        request: Request<ListAgentModelsRequest>,
    ) -> Result<Response<ListAgentModelsResponse>, Status>;

    async fn list_subagents(
        &self,
        request: Request<ListSubagentsRequest>,
    ) -> Result<Response<ListSubagentsResponse>, Status>;
}

/// Thin `CatalogService` adapter over a [`CatalogHandler`].
pub struct CatalogServiceImpl<H> {
    host: Arc<H>,
}

impl<H> CatalogServiceImpl<H> {
    #[must_use]
    pub fn new(host: Arc<H>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl<H: CatalogHandler + 'static> CatalogService for CatalogServiceImpl<H> {
    async fn list_tools(
        &self,
        request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        self.host.list_tools(request).await
    }

    async fn list_agents(
        &self,
        request: Request<ListAgentsRequest>,
    ) -> Result<Response<ListAgentsResponse>, Status> {
        self.host.list_agents(request).await
    }

    async fn list_agent_models(
        &self,
        request: Request<ListAgentModelsRequest>,
    ) -> Result<Response<ListAgentModelsResponse>, Status> {
        self.host.list_agent_models(request).await
    }

    async fn list_subagents(
        &self,
        request: Request<ListSubagentsRequest>,
    ) -> Result<Response<ListSubagentsResponse>, Status> {
        self.host.list_subagents(request).await
    }
}
