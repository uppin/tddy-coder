//! `exec_tools.ExecToolService` — family L, served from this crate.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolChunk, ExecuteToolRequest, ExecuteToolResponse,
    ListExecToolsRequest, ListExecToolsResponse, ListSessionToolCallsRequest,
    ListSessionToolCallsResponse,
};
use tddy_worktree_service::stream::MpscResultStream;

/// What the four exec-tool RPCs need from the host process.
#[async_trait]
pub trait ExecToolHandler: Send + Sync {
    async fn execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<ExecuteToolResponse>, Status>;

    async fn stream_execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<MpscResultStream<ExecuteToolChunk>>, Status>;

    async fn list_exec_tools(
        &self,
        request: Request<ListExecToolsRequest>,
    ) -> Result<Response<ListExecToolsResponse>, Status>;

    async fn list_session_tool_calls(
        &self,
        request: Request<ListSessionToolCallsRequest>,
    ) -> Result<Response<ListSessionToolCallsResponse>, Status>;
}

/// Thin `ExecToolService` adapter over an [`ExecToolHandler`].
pub struct ExecToolServiceImpl<H> {
    host: Arc<H>,
}

impl<H> ExecToolServiceImpl<H> {
    #[must_use]
    pub fn new(host: Arc<H>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl<H: ExecToolHandler + 'static> ExecToolService for ExecToolServiceImpl<H> {
    type StreamExecuteToolStream = MpscResultStream<ExecuteToolChunk>;

    async fn execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<ExecuteToolResponse>, Status> {
        self.host.execute_tool(request).await
    }

    async fn stream_execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<Self::StreamExecuteToolStream>, Status> {
        self.host.stream_execute_tool(request).await
    }

    async fn list_exec_tools(
        &self,
        request: Request<ListExecToolsRequest>,
    ) -> Result<Response<ListExecToolsResponse>, Status> {
        self.host.list_exec_tools(request).await
    }

    async fn list_session_tool_calls(
        &self,
        request: Request<ListSessionToolCallsRequest>,
    ) -> Result<Response<ListSessionToolCallsResponse>, Status> {
        self.host.list_session_tool_calls(request).await
    }
}
