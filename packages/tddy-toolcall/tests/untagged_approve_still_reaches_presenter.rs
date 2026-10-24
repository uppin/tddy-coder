//! Acceptance: an `approve` without a conversation id still goes to the workflow presenter, even on
//! a listener that has a conversation-approval handler bound. One test per file:
//! `start_toolcall_listener`'s socket path is keyed only by process id.

use async_trait::async_trait;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tddy_toolcall::toolcall::{
    dispatch_toolcall, start_toolcall_listener_with_handlers, ConversationApprovalHandler,
    ListenerHandlers, ToolCallRequest, ToolCallResponse,
};

const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// Counts how often it is asked; it must never be asked for an untagged approve.
#[derive(Default)]
struct CountingApprovalHandler {
    calls: Mutex<usize>,
}

impl CountingApprovalHandler {
    fn call_count(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

#[async_trait]
impl ConversationApprovalHandler for CountingApprovalHandler {
    async fn approve(
        &self,
        _conversation_id: &str,
        _tool_name: &str,
        _input: &serde_json::Value,
    ) -> Result<bool, String> {
        *self.calls.lock().unwrap() += 1;
        Ok(false)
    }
}

/// **untagged_approve_still_reaches_presenter**: an `approve` carrying no conversation id is
/// relayed to the workflow presenter exactly as before, and the presenter's decision is returned.
#[tokio::test]
async fn untagged_approve_still_reaches_presenter_acceptance() {
    // Given a real listener with a conversation-approval handler bound, and a stand-in presenter
    // that records the tool it was asked about and allows it
    let tddy_data_dir =
        std::env::temp_dir().join(format!("tddy-toolcall-approval-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tddy_data_dir).unwrap();
    let conversation_handler = Arc::new(CountingApprovalHandler::default());
    let handlers = ListenerHandlers {
        conversation_approval: Some(conversation_handler.clone()),
        ..ListenerHandlers::default()
    };
    let (socket_path, tool_rx) =
        start_toolcall_listener_with_handlers(None, None, tddy_data_dir, handlers)
            .expect("start toolcall listener");
    let (asked_tx, asked_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for request in tool_rx.iter().take(1) {
            if let ToolCallRequest::Approve {
                tool_name,
                response_tx,
                ..
            } = request
            {
                let _ = asked_tx.send(tool_name);
                let _ = response_tx.send(ToolCallResponse::ApproveResult { allow: true });
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

    // Then the presenter was asked about that tool and its allow comes back
    assert_eq!(
        asked_rx
            .recv_timeout(CALL_TIMEOUT)
            .expect("the presenter must be asked"),
        "Write"
    );
    assert_eq!(response["status"], "ok");
    assert_eq!(response["decision"], "allow");

    // And the conversation-approval handler was never consulted
    assert_eq!(conversation_handler.call_count(), 0);
}
