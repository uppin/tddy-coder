#[test]
fn names_the_service_family_l_moves_to() {
    struct EmptyExec;
    #[async_trait::async_trait]
    impl tddy_service::proto::exec_tools::ExecToolService for EmptyExec {
        type StreamExecuteToolStream = futures_util::stream::Empty<
            Result<tddy_service::proto::exec_tools::ExecuteToolChunk, tddy_rpc::Status>,
        >;
        async fn execute_tool(
            &self,
            _request: tddy_rpc::Request<tddy_service::proto::exec_tools::ExecuteToolRequest>,
        ) -> Result<
            tddy_rpc::Response<tddy_service::proto::exec_tools::ExecuteToolResponse>,
            tddy_rpc::Status,
        > {
            Err(tddy_rpc::Status::unimplemented("stub"))
        }
        async fn stream_execute_tool(
            &self,
            _request: tddy_rpc::Request<tddy_service::proto::exec_tools::ExecuteToolRequest>,
        ) -> Result<tddy_rpc::Response<Self::StreamExecuteToolStream>, tddy_rpc::Status> {
            Err(tddy_rpc::Status::unimplemented("stub"))
        }
        async fn list_exec_tools(
            &self,
            _request: tddy_rpc::Request<tddy_service::proto::exec_tools::ListExecToolsRequest>,
        ) -> Result<
            tddy_rpc::Response<tddy_service::proto::exec_tools::ListExecToolsResponse>,
            tddy_rpc::Status,
        > {
            Ok(tddy_rpc::Response::new(
                tddy_service::proto::exec_tools::ListExecToolsResponse { tools: vec![] },
            ))
        }
        async fn list_session_tool_calls(
            &self,
            _request: tddy_rpc::Request<
                tddy_service::proto::exec_tools::ListSessionToolCallsRequest,
            >,
        ) -> Result<
            tddy_rpc::Response<tddy_service::proto::exec_tools::ListSessionToolCallsResponse>,
            tddy_rpc::Status,
        > {
            Ok(tddy_rpc::Response::new(
                tddy_service::proto::exec_tools::ListSessionToolCallsResponse {
                    tool_calls: vec![],
                },
            ))
        }
        async fn conversation_worktree(
            &self,
            _request: tddy_rpc::Request<
                tddy_service::proto::exec_tools::ConversationWorktreeRequest,
            >,
        ) -> Result<
            tddy_rpc::Response<tddy_service::proto::exec_tools::ConversationWorktreeResponse>,
            tddy_rpc::Status,
        > {
            Err(tddy_rpc::Status::unimplemented("stub"))
        }
        async fn request_approval(
            &self,
            _request: tddy_rpc::Request<tddy_service::proto::exec_tools::ApprovalRequest>,
        ) -> Result<
            tddy_rpc::Response<tddy_service::proto::exec_tools::ApprovalResponse>,
            tddy_rpc::Status,
        > {
            Err(tddy_rpc::Status::unimplemented("stub"))
        }
    }
    assert_eq!(
        super::build_exec_tool_entry(EmptyExec).name,
        "exec_tools.ExecToolService"
    );
}
