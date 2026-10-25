//! Regression: on the plain listener, an `approve` that names no conversation is decided by the
//! workflow presenter, and the presenter's own answer is what the caller gets back.
//!
//! The test that pinned this was deleted with the conversation-approval hook (#coagents 7/8), whose
//! tagged branch sat in front of this path. One test per file: `start_toolcall_listener`'s socket
//! path is keyed only by process id.

use serde_json::{json, Value};
use std::time::Duration;
use tddy_toolcall::toolcall::{
    dispatch_toolcall, start_toolcall_listener, ToolCallRequest, ToolCallResponse,
};

const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// **an_untagged_approve_is_decided_by_the_presenter**: the presenter is asked about the very tool
/// and input the caller named, and its denial — not a default — comes back.
#[tokio::test]
async fn an_untagged_approve_is_decided_by_the_presenter() {
    // Given a plain listener (no conversation handler of any kind), and a stand-in presenter that
    // records what it was asked and denies it
    let tddy_data_dir = std::env::temp_dir().join(format!(
        "tddy-toolcall-untagged-approve-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&tddy_data_dir).unwrap();
    let (socket_path, tool_rx) =
        start_toolcall_listener(None, None, tddy_data_dir).expect("start toolcall listener");
    let (asked_tx, asked_rx) = std::sync::mpsc::channel::<(String, Value)>();
    std::thread::spawn(move || {
        for request in tool_rx.iter() {
            if let ToolCallRequest::Approve {
                tool_name,
                input,
                response_tx,
            } = request
            {
                let _ = asked_tx.send((tool_name, input));
                let _ = response_tx.send(ToolCallResponse::ApproveResult { allow: false });
                return;
            }
        }
    });

    // When an approval is requested with no conversation id
    let response = tokio::time::timeout(
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

    // Then the presenter was asked about that tool with that input
    let (asked_tool, asked_input) = asked_rx
        .recv_timeout(CALL_TIMEOUT)
        .expect("the presenter must be asked");
    assert_eq!(asked_tool, "Write");
    assert_eq!(asked_input, json!({"path": "README.md"}));

    // And the presenter's denial is the answer the caller receives
    assert_eq!(response["status"], "ok");
    assert_eq!(response["decision"], "deny");
}
