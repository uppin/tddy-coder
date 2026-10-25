//! The daemon's exec-tool service on a real loopback port, for the wire tests of a session's tool
//! calls over the daemon's HTTP relay.
//!
//! The port serves the mount the daemon builds: `connect_router` over an `RpcBridge` over a
//! `MultiRpcService`, with a generated `exec_tools.ExecToolService` implementation registered in it.
//! That implementation is reached only through the generated handler, which decodes each body with
//! the prost type of its method. A body that type cannot decode never reaches the implementation, so
//! a test sees a request only when the wire carried a protobuf message the daemon reads.

// Each test binary that includes this module uses a different part of it.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_connectrpc::connect_router;
use tddy_rpc::{
    MultiRpcService, Request, Response, RpcBridge, RpcService, ServiceEntry, Status, Streaming,
};
use tddy_service::proto::exec_tools::{
    ApprovalRequest, ApprovalResponse, ConversationWorktreeRequest, ConversationWorktreeResponse,
    ExecToolService, ExecToolServiceServer, ExecuteToolChunk, ExecuteToolRequest,
    ExecuteToolResponse, ListExecToolsRequest, ListExecToolsResponse, ListSessionToolCallsRequest,
    ListSessionToolCallsResponse, ToolDef,
};

/// The coordinate the daemon mounts the exec-tool service at.
pub const EXEC_TOOL_SERVICE: &str = "exec_tools.ExecToolService";

/// The session the environment names, and the identity its tool calls carry.
pub const SESSION_ID: &str = "sess-http-1";
pub const SESSION_TOKEN: &str = "tok-http";
pub const DAEMON_INSTANCE_ID: &str = "codebase-host";

/// The exec-tool service the daemon serves, reduced to what a session's tool calls reach. It records
/// every request the generated handler decoded, and answers `ExecuteTool` and `ListExecTools` from
/// what the test set. Every other method refuses, because no test in these files reaches it.
pub struct ExecToolsDaemon {
    execute_answer: ExecuteToolResponse,
    tools: Vec<String>,
    conversation_answer: ConversationWorktreeResponse,
    executed: Mutex<Vec<ExecuteToolRequest>>,
    listed: Mutex<Vec<ListExecToolsRequest>>,
    conversations: Mutex<Vec<ConversationWorktreeRequest>>,
}

impl ExecToolsDaemon {
    /// A daemon that answers `ExecuteTool` with `execute_answer` and `ListExecTools` with `tools`.
    pub fn answering(execute_answer: ExecuteToolResponse, tools: &[&str]) -> Self {
        Self {
            execute_answer,
            tools: tools.iter().map(|tool| tool.to_string()).collect(),
            conversation_answer: ConversationWorktreeResponse::default(),
            executed: Mutex::new(Vec::new()),
            listed: Mutex::new(Vec::new()),
            conversations: Mutex::new(Vec::new()),
        }
    }

    /// The same daemon, answering `ConversationWorktree` with `answer`.
    pub fn with_conversation_answer(mut self, answer: ConversationWorktreeResponse) -> Self {
        self.conversation_answer = answer;
        self
    }

    /// The `ExecuteTool` requests the generated handler decoded, in arrival order.
    pub fn decoded_executes(&self) -> Vec<ExecuteToolRequest> {
        self.executed.lock().expect("recorded executes").clone()
    }

    /// The `ListExecTools` requests the generated handler decoded, in arrival order.
    pub fn decoded_lists(&self) -> Vec<ListExecToolsRequest> {
        self.listed.lock().expect("recorded lists").clone()
    }

    /// The `ConversationWorktree` requests the generated handler decoded, in arrival order.
    pub fn decoded_conversations(&self) -> Vec<ConversationWorktreeRequest> {
        self.conversations
            .lock()
            .expect("recorded conversations")
            .clone()
    }
}

#[async_trait]
impl ExecToolService for ExecToolsDaemon {
    type StreamExecuteToolStream = Streaming<ExecuteToolChunk>;

    async fn execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<ExecuteToolResponse>, Status> {
        self.executed
            .lock()
            .expect("recorded executes")
            .push(request.into_inner());
        Ok(Response::new(self.execute_answer.clone()))
    }

    async fn stream_execute_tool(
        &self,
        _request: Request<ExecuteToolRequest>,
    ) -> Result<Response<Self::StreamExecuteToolStream>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn list_exec_tools(
        &self,
        request: Request<ListExecToolsRequest>,
    ) -> Result<Response<ListExecToolsResponse>, Status> {
        self.listed
            .lock()
            .expect("recorded lists")
            .push(request.into_inner());
        Ok(Response::new(ListExecToolsResponse {
            tools: self
                .tools
                .iter()
                .map(|name| ToolDef {
                    name: name.clone(),
                    ..Default::default()
                })
                .collect(),
        }))
    }

    async fn list_session_tool_calls(
        &self,
        _request: Request<ListSessionToolCallsRequest>,
    ) -> Result<Response<ListSessionToolCallsResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn conversation_worktree(
        &self,
        request: Request<ConversationWorktreeRequest>,
    ) -> Result<Response<ConversationWorktreeResponse>, Status> {
        self.conversations
            .lock()
            .expect("recorded conversations")
            .push(request.into_inner());
        Ok(Response::new(self.conversation_answer.clone()))
    }

    async fn request_approval(
        &self,
        _request: Request<ApprovalRequest>,
    ) -> Result<Response<ApprovalResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }
}

/// A daemon serving `ExecToolService` on a real loopback port, mounted the way the daemon mounts it.
pub struct ADaemonOnARealPort {
    port: u16,
    pub daemon: Arc<ExecToolsDaemon>,
    _serving: tokio::task::JoinHandle<()>,
}

impl ADaemonOnARealPort {
    pub async fn serving(daemon: ExecToolsDaemon) -> Self {
        let daemon = Arc::new(daemon);
        let router = connect_router(RpcBridge::new(MultiRpcService::new(vec![ServiceEntry {
            name: EXEC_TOOL_SERVICE,
            service: Arc::new(ExecToolServiceServer::from_arc(Arc::clone(&daemon)))
                as Arc<dyn RpcService>,
        }])));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the daemon endpoint");
        let port = listener
            .local_addr()
            .expect("daemon endpoint local addr")
            .port();
        let serving = tokio::spawn(async move {
            // Not swallowed: a harness that failed to serve records no request, which would be
            // reported as a wire bug.
            axum::serve(listener, router)
                .await
                .expect("the daemon endpoint served");
        });
        Self {
            port,
            daemon,
            _serving: serving,
        }
    }

    /// The daemon's RPC base, as a session's environment carries its relay URL: the `/rpc` mount.
    pub fn rpc_base_url(&self) -> String {
        format!("http://127.0.0.1:{}/rpc", self.port)
    }
}

/// Points this process's session tool transport at `daemon_url`, as a session's environment does for
/// a daemon it reaches over HTTP. Clears the variables that would select another transport first.
pub fn a_session_reaching_its_daemon_over_http(daemon_url: &str) {
    for key in [
        "TDDY_SANDBOX_TOOL_IPC",
        "TDDY_REMOTE_DAEMON_SOCKET",
        "TDDY_REMOTE_LIVEKIT_URL",
        "TDDY_REMOTE_LIVEKIT_ROOM",
        "TDDY_REMOTE_LIVEKIT_TOKEN",
        "TDDY_REMOTE_SERVER_IDENTITY",
    ] {
        std::env::remove_var(key);
    }
    std::env::set_var("TDDY_REMOTE_SESSION_ID", SESSION_ID);
    std::env::set_var("TDDY_REMOTE_SESSION_TOKEN", SESSION_TOKEN);
    std::env::set_var("TDDY_REMOTE_DAEMON_INSTANCE_ID", DAEMON_INSTANCE_ID);
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", daemon_url);
}
