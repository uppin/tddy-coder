// Appended to OUT_DIR/exec_tools.rs when prost-build omits the generated adapter.
// TODO: remove once tddy-codegen reliably emits this block for exec_tools.proto.

use crate::proto::tonic_exec_tools::exec_tool_service_server;
use tddy_service::to_tonic_status;

/// Adapter to use ExecToolService with tonic gRPC server.
pub struct ExecToolServiceTonicAdapter<T> {
    inner: std::sync::Arc<T>,
}

impl<T> ExecToolServiceTonicAdapter<T> {
    pub fn new(inner: std::sync::Arc<T>) -> Self {
        Self { inner }
    }
}

#[tonic::async_trait]
impl<T> exec_tool_service_server::ExecToolService for ExecToolServiceTonicAdapter<T>
where
    T: ExecToolService,
    T::StreamExecuteToolStream: 'static,
{
    async fn execute_tool(
        &self,
        request: tonic::Request<ExecuteToolRequest>,
    ) -> Result<tonic::Response<ExecuteToolResponse>, tonic::Status> {
        let resp = ExecToolService::execute_tool(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    type StreamExecuteToolStream = std::pin::Pin<
        Box<
            dyn futures_util::Stream<Item = Result<ExecuteToolChunk, tonic::Status>> + Send,
        >,
    >;

    #[allow(clippy::result_large_err)]
    async fn stream_execute_tool(
        &self,
        request: tonic::Request<ExecuteToolRequest>,
    ) -> Result<tonic::Response<Self::StreamExecuteToolStream>, tonic::Status> {
        let resp = ExecToolService::stream_execute_tool(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        let outbound = resp.into_inner().map(|item| item.map_err(to_tonic_status));
        Ok(tonic::Response::new(Box::pin(outbound)))
    }

    async fn list_exec_tools(
        &self,
        request: tonic::Request<ListExecToolsRequest>,
    ) -> Result<tonic::Response<ListExecToolsResponse>, tonic::Status> {
        let resp = ExecToolService::list_exec_tools(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    async fn list_session_tool_calls(
        &self,
        request: tonic::Request<ListSessionToolCallsRequest>,
    ) -> Result<tonic::Response<ListSessionToolCallsResponse>, tonic::Status> {
        let resp = ExecToolService::list_session_tool_calls(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }
}
