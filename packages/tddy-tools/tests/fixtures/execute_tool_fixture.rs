//! Test fixture: hosts a fake `connection.ConnectionService/ExecuteTool` handler over its own
//! stdin/stdout, exercised by `tests/session_tool_stdio_rpc_dispatch.rs`. Not test code itself —
//! support for that test, in the spirit of `tddy-stdio`'s own `stdio-echo-fixture`
//! (`tddy-stdio/tests/fixtures/echo_child.rs`).
//!
//! Echoes the request's `args_json` back as the response's `result_json`, verbatim — enough to
//! prove a tool-call payload round-trips over the stdio RPC channel without truncation, without
//! needing a real daemon/sandbox on the other end.
//!
//! Before it can be usefully called, it calls back into the parent's
//! `parent.FixtureReadyService/Ready` over the same pipe pair (the reverse-call pattern
//! `echo_child.rs` demonstrates). That announcement is the parent's readiness signal: without it
//! the parent would have to guess how long fork/exec, dynamic linking and tokio start-up take, and
//! fold that guess into the budget meant to measure a single tool call.

use async_trait::async_trait;
use prost::Message;
use tddy_rpc::{RpcClientTransport, RpcMessage, RpcResult, RpcService};
use tddy_service::proto::connection::{ExecuteToolRequest, ExecuteToolResponse};

struct FakeExecuteToolService;

#[async_trait]
impl RpcService for FakeExecuteToolService {
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        assert_eq!(service, "connection.ConnectionService");
        assert_eq!(method, "ExecuteTool");
        let request = ExecuteToolRequest::decode(message.payload.as_ref())
            .expect("decode ExecuteToolRequest");
        let response = ExecuteToolResponse {
            result_json: request.args_json,
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        };
        RpcResult::Unary(Ok(response.encode_to_vec()))
    }
}

// current_thread: a tiny, single-connection test fixture — the single-threaded runtime is
// sufficient, avoiding a "rt-multi-thread" requirement for a [[bin]] target that only ever runs
// under `cargo test` (mirrors tddy-stdio's stdio-echo-fixture).
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (client, endpoint) = tddy_stdio::StdioEndpoint::from_process_stdio(FakeExecuteToolService);
    let run_handle = tokio::spawn(endpoint.run());

    client
        .call_unary("parent.FixtureReadyService", "Ready", Vec::new())
        .await
        .expect("announce readiness to the parent");

    let _ = run_handle.await;
}
