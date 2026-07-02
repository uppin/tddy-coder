//! Acceptance: `ask`/`approve` toolcall verbs relayed over the new `tddy-rpc`/`tddy-stdio`
//! framing — see `toolcall_stdio_relay_submit_acceptance.rs` for the migration context. One test
//! per file: `start_toolcall_listener`'s socket path is keyed only by process id.

use serde_json::json;
use std::time::Duration;
use tddy_core::toolcall::{start_toolcall_listener, ToolCallRequest, ToolCallResponse};
use tddy_tools::toolcall_client::dispatch_toolcall;

const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// **ask_and_approve_round_trip_over_the_stdio_rpc_transport**: `ask` and `approve` requests
/// block until the presenter (simulated here, as the real presenter would) answers via the
/// `ToolCallRequest` channel, and the answer comes back over the new transport unchanged.
#[tokio::test]
async fn ask_and_approve_round_trip_over_the_stdio_rpc_transport() {
    // Given a real toolcall listener, with a stand-in "presenter" answering questions/approvals
    // exactly as `Presenter::poll_tool_calls` would
    let tddy_data_dir =
        std::env::temp_dir().join(format!("tddy-toolcall-stdio-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tddy_data_dir).unwrap();
    let (socket_path, tool_rx) =
        start_toolcall_listener(None, None, tddy_data_dir).expect("start toolcall listener");
    std::thread::spawn(move || {
        for request in tool_rx.iter().take(2) {
            match request {
                ToolCallRequest::Ask { response_tx, .. } => {
                    let _ = response_tx.send(ToolCallResponse::AskAnswer {
                        answers: "42".to_string(),
                    });
                }
                ToolCallRequest::Approve { response_tx, .. } => {
                    let _ = response_tx.send(ToolCallResponse::ApproveResult { allow: true });
                }
                ToolCallRequest::SubmitActivity { .. } => {}
            }
        }
    });

    // When asking a clarifying question over the new stdio-RPC transport
    let ask_response = tokio::time::timeout(
        CALL_TIMEOUT,
        dispatch_toolcall(
            &socket_path,
            json!({
                "type": "ask",
                "questions": [{
                    "header": "Meaning",
                    "question": "what is the meaning of life?",
                    "options": [],
                }],
            }),
        ),
    )
    .await
    .expect("ask relay timed out")
    .expect("ask relay succeeds");

    // Then the presenter's answer comes back over the new transport
    assert_eq!(ask_response["status"], "ok");
    assert_eq!(ask_response["answers"], "42");

    // When requesting tool approval over the new stdio-RPC transport
    let approve_response = tokio::time::timeout(
        CALL_TIMEOUT,
        dispatch_toolcall(
            &socket_path,
            json!({
                "type": "approve",
                "tool_name": "Write",
                "input": {"path": "README.md"},
            }),
        ),
    )
    .await
    .expect("approve relay timed out")
    .expect("approve relay succeeds");

    // Then the presenter's decision comes back over the new transport
    assert_eq!(approve_response["status"], "ok");
    assert_eq!(approve_response["decision"], "allow");
}
