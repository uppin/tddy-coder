//! A session agent's `tddy-tools --mcp` renews its own token before the token runs out, so its tool
//! calls keep working past the TTL the token was minted with.
//!
//! The daemon here is a stub on a real Unix socket, the transport an orchestrator uses. It accepts
//! one token at a time. Renewing hands out the next token and retires the old one, which stands in
//! for the old token reaching its expiry.
//!
//! Incident: docs/dev/1-WIP (orchestrator 01a20f10 lost every tool when its 24 h token expired).

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serial_test::serial;
use tddy_rpc::{Request, Response, Status, Streaming};
use tddy_service::proto::exec_tools::{
    ApprovalRequest, ApprovalResponse, ConversationWorktreeRequest, ConversationWorktreeResponse,
    ExecToolService, ExecToolServiceServer, ExecuteToolChunk, ExecuteToolRequest,
    ExecuteToolResponse, ListExecToolsRequest, ListExecToolsResponse, ListSessionToolCallsRequest,
    ListSessionToolCallsResponse,
};
use tddy_session_tool_client::dispatch_session_tool;
use tddy_session_tool_client::session_token::{
    renew_session_token_if_due, RenewalOutcome, RENEW_AGENT_SESSION_TOKEN_TOOL,
};

const SESSION_ID: &str = "01a20f10-orchestrator";
const SPAWN_TOKEN: &str = "agent-token-1";
const RENEWED_TOKEN: &str = "agent-token-2";

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs()
}

/// A daemon that accepts exactly one valid agent token and renews it on request.
struct OneValidTokenDaemon {
    valid: Mutex<String>,
    renewals: Mutex<Vec<ExecuteToolRequest>>,
}

impl OneValidTokenDaemon {
    fn holding(token: &str) -> Self {
        Self {
            valid: Mutex::new(token.to_string()),
            renewals: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ExecToolService for OneValidTokenDaemon {
    type StreamExecuteToolStream = Streaming<ExecuteToolChunk>;

    async fn execute_tool(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<Response<ExecuteToolResponse>, Status> {
        let req = request.into_inner();
        let mut valid = self.valid.lock().expect("valid token");
        if req.session_token != *valid {
            return Err(Status::unauthenticated("the session token has expired"));
        }
        if req.tool_name == RENEW_AGENT_SESSION_TOKEN_TOOL {
            self.renewals.lock().expect("renewals").push(req.clone());
            *valid = RENEWED_TOKEN.to_string();
            return Ok(Response::new(ExecuteToolResponse {
                result_json: serde_json::json!({
                    "session_token": RENEWED_TOKEN,
                    "expires_at_unix": now_unix() + 86_400,
                })
                .to_string(),
                ..Default::default()
            }));
        }
        Ok(Response::new(ExecuteToolResponse {
            result_json: r#"{"content":"ok"}"#.to_string(),
            ..Default::default()
        }))
    }

    async fn stream_execute_tool(
        &self,
        _request: Request<ExecuteToolRequest>,
    ) -> Result<Response<Self::StreamExecuteToolStream>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn list_exec_tools(
        &self,
        _request: Request<ListExecToolsRequest>,
    ) -> Result<Response<ListExecToolsResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn list_session_tool_calls(
        &self,
        _request: Request<ListSessionToolCallsRequest>,
    ) -> Result<Response<ListSessionToolCallsResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn conversation_worktree(
        &self,
        _request: Request<ConversationWorktreeRequest>,
    ) -> Result<Response<ConversationWorktreeResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn request_approval(
        &self,
        _request: Request<ApprovalRequest>,
    ) -> Result<Response<ApprovalResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }
}

/// Serve `daemon` on a fresh Unix socket, one endpoint per connection, as the agent-tool socket is.
fn a_daemon_socket(tag: &str, daemon: Arc<OneValidTokenDaemon>) -> std::path::PathBuf {
    let socket_path = tddy_sandbox::SandboxSpec::short_ipc_socket_path(tag);
    let _ = std::fs::remove_file(&socket_path);
    let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind the daemon socket");
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let daemon = Arc::clone(&daemon);
            tokio::spawn(async move {
                let (read_half, write_half) = tokio::io::split(stream);
                let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                    read_half,
                    write_half,
                    ExecToolServiceServer::from_arc(daemon),
                    tddy_rpc::RequestTransport::UnixSocket,
                );
                endpoint.run().await;
            });
        }
    });
    socket_path
}

/// The environment a host-run agent's `tddy-tools --mcp` gets: the daemon socket and its token.
fn an_agent_reaching_its_daemon_over(socket: &std::path::Path, token: &str) {
    for key in [
        "TDDY_SANDBOX_TOOL_IPC",
        "TDDY_REMOTE_DAEMON_URL",
        "TDDY_REMOTE_LIVEKIT_URL",
        "TDDY_REMOTE_LIVEKIT_ROOM",
        "TDDY_REMOTE_LIVEKIT_TOKEN",
        "TDDY_REMOTE_SERVER_IDENTITY",
    ] {
        std::env::remove_var(key);
    }
    std::env::set_var("TDDY_REMOTE_SESSION_ID", SESSION_ID);
    std::env::set_var("TDDY_REMOTE_SESSION_TOKEN", token);
    std::env::set_var("TDDY_REMOTE_DAEMON_INSTANCE_ID", "LT-agent-host");
    std::env::set_var("TDDY_REMOTE_DAEMON_SOCKET", socket);
}

#[tokio::test]
#[serial]
async fn a_token_near_expiry_is_renewed_and_tool_calls_keep_working_past_its_original_expiry() {
    // Given an agent whose spawn token has ten minutes left
    let daemon = Arc::new(OneValidTokenDaemon::holding(SPAWN_TOKEN));
    let socket = a_daemon_socket("renew-near-expiry", Arc::clone(&daemon));
    an_agent_reaching_its_daemon_over(&socket, SPAWN_TOKEN);
    let spawn_token_expiry = now_unix() + 600;

    // When the renewal check runs with six hours of margin
    let outcome =
        renew_session_token_if_due(Some(spawn_token_expiry), Duration::from_secs(6 * 3600))
            .await
            .expect("the renewal succeeds");

    // Then the token was renewed under the spawn token, addressed to the agent's own daemon
    assert!(
        matches!(outcome, RenewalOutcome::Renewed { expires_at_unix } if expires_at_unix > spawn_token_expiry)
    );
    let renewal = daemon.renewals.lock().expect("renewals")[0].clone();
    assert_eq!(
        (
            renewal.session_id.as_str(),
            renewal.daemon_instance_id.as_str()
        ),
        (SESSION_ID, "")
    );

    // And a tool call made after the spawn token stopped working still succeeds
    let answer = dispatch_session_tool("Read", serde_json::json!({"path": "README.md"})).await;
    assert_eq!(answer, r#"{"content":"ok"}"#);
}

#[tokio::test]
#[serial]
async fn a_token_with_plenty_of_life_left_is_not_renewed() {
    // Given an agent whose token has a day left
    let daemon = Arc::new(OneValidTokenDaemon::holding(SPAWN_TOKEN));
    let socket = a_daemon_socket("renew-not-due", Arc::clone(&daemon));
    an_agent_reaching_its_daemon_over(&socket, SPAWN_TOKEN);

    // When the renewal check runs with six hours of margin
    let outcome =
        renew_session_token_if_due(Some(now_unix() + 86_000), Duration::from_secs(6 * 3600))
            .await
            .expect("the check succeeds");

    // Then nothing is asked of the daemon
    assert_eq!(outcome, RenewalOutcome::NotDue);
    assert!(daemon.renewals.lock().expect("renewals").is_empty());
}
