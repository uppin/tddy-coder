//! Acceptance: a conversation-tagged `approve` is decided by that conversation's own
//! [`ConversationApprovalHandler`], never relayed to the workflow presenter. One test per file:
//! `start_toolcall_listener`'s socket path is keyed only by process id.

use async_trait::async_trait;
use serde_json::json;
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tddy_toolcall::toolcall::{
    dispatch_toolcall, start_toolcall_listener_with_handlers, ConversationApprovalHandler,
    ListenerHandlers,
};

const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// Records the conversation and tool it was asked about, and allows every request.
#[derive(Default)]
struct RecordingApprovalHandler {
    calls: Mutex<Vec<(String, String)>>,
}

impl RecordingApprovalHandler {
    fn calls(&self) -> Vec<(String, String)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl ConversationApprovalHandler for RecordingApprovalHandler {
    async fn approve(
        &self,
        conversation_id: &str,
        tool_name: &str,
        _input: &serde_json::Value,
    ) -> Result<bool, String> {
        self.calls
            .lock()
            .unwrap()
            .push((conversation_id.to_string(), tool_name.to_string()));
        Ok(true)
    }
}

/// **conversation_approval_tagged_routes_to_handler**: an `approve` tagged with a conversation id
/// is answered by the listener's conversation-approval handler, and the workflow presenter is
/// never asked to decide it.
#[tokio::test]
async fn conversation_approval_tagged_routes_to_handler_acceptance() {
    // Given a real listener whose conversation-approval handler records what it is asked and allows it
    let tddy_data_dir =
        std::env::temp_dir().join(format!("tddy-toolcall-approval-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tddy_data_dir).unwrap();
    let handler = Arc::new(RecordingApprovalHandler::default());
    let handlers = ListenerHandlers {
        conversation_approval: Some(handler.clone()),
        ..ListenerHandlers::default()
    };
    let (socket_path, tool_rx) =
        start_toolcall_listener_with_handlers(None, None, tddy_data_dir, handlers)
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

    // Then the conversation's own handler decided it, and the decision is an allow
    assert_eq!(response["status"], "ok");
    assert_eq!(response["decision"], "allow");
    assert_eq!(
        handler.calls(),
        vec![("conv-1".to_string(), "Bash".to_string())],
        "the handler must be asked about the tagged conversation and tool"
    );

    // And the workflow presenter received no approval request
    match tool_rx.try_recv() {
        Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => {}
        Ok(request) => panic!("the presenter must not be asked, got {request:?}"),
    }
}
