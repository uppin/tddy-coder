//! Acceptance: a conversation-tagged `approve` on a listener with no conversation-approval handler
//! is refused, and is never relayed to the workflow presenter. One test per file:
//! `start_toolcall_listener`'s socket path is keyed only by process id.

use serde_json::json;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;
use tddy_toolcall::toolcall::{
    dispatch_toolcall, start_toolcall_listener_with_handlers, ListenerHandlers,
};

const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// **conversation_approval_fails_closed_without_handler**: with no conversation-approval handler
/// bound, a conversation-tagged `approve` is refused with an error and is not handed to the
/// workflow presenter, so no approval can be granted by accident.
#[tokio::test]
async fn conversation_approval_fails_closed_without_handler_acceptance() {
    // Given a real listener with no conversation-approval handler bound
    let tddy_data_dir =
        std::env::temp_dir().join(format!("tddy-toolcall-approval-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tddy_data_dir).unwrap();
    let (socket_path, tool_rx) = start_toolcall_listener_with_handlers(
        None,
        None,
        tddy_data_dir,
        ListenerHandlers::default(),
    )
    .expect("start toolcall listener");

    // When a conversation requests approval of its own tool call, tagged with its conversation id
    let response = tokio::time::timeout(
        CALL_TIMEOUT,
        dispatch_toolcall(
            &socket_path,
            json!({
                "type": "approve",
                "tool_name": "Bash",
                "input": {"command": "ls"},
                "conversation_id": "conv-1",
            }),
        ),
    )
    .await
    .expect("approve relay timed out")
    .expect("approve relay succeeds");

    // Then the request is refused with an error that carries a reason, not allowed
    assert_eq!(response["status"], "error");
    assert!(
        !response["message"].as_str().unwrap_or_default().is_empty(),
        "expected an actionable refusal message, got: {response}"
    );
    assert!(response.get("decision").is_none());

    // And the workflow presenter received no approval request
    match tool_rx.try_recv() {
        Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => {}
        Ok(request) => panic!("the presenter must not be asked, got {request:?}"),
    }
}
