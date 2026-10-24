//! Acceptance: a tool-use approval raised by a headless conversation is decided by that
//! conversation's own decision point, and an allow from it reaches Claude as an allow.
//!
//! `PermissionServer` reads `TDDY_SOCKET` and `TDDY_CONVERSATION_ID` when it is constructed, and both
//! are process-wide, so this file holds one test. The listener's socket path is keyed only by process
//! id, which is the other reason a file holds a single test.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tddy_core::toolcall::{
    start_toolcall_listener_with_handlers, ConversationApprovalHandler, ListenerHandlers,
};
use tddy_tools::server::PermissionServer;

/// Records the conversation and tool it was asked about, and allows the request.
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
        _input: &Value,
    ) -> Result<bool, String> {
        self.calls
            .lock()
            .unwrap()
            .push((conversation_id.to_string(), tool_name.to_string()));
        Ok(true)
    }
}

/// **tagged_approval_is_decided_by_its_conversation**: a Bash permission prompt raised by a
/// conversation is sent to the listener tagged with that conversation, and the handler's allow is
/// the reply Claude receives.
#[test]
fn tagged_bash_approval_is_allowed_by_its_conversation() {
    // Given a listener whose conversation-approval handler records what it is asked and allows it
    let data_dir = tempfile::tempdir().expect("temp dir");
    let handler = Arc::new(RecordingApprovalHandler::default());
    let handlers = ListenerHandlers {
        conversation_approval: Some(handler.clone()),
        ..ListenerHandlers::default()
    };
    let (socket_path, _presenter_rx) =
        start_toolcall_listener_with_handlers(None, None, data_dir.path().to_path_buf(), handlers)
            .expect("start toolcall listener");

    // And a managed conversation's tool server, wired to that listener
    std::env::set_var("TDDY_SOCKET", &socket_path);
    std::env::set_var("TDDY_CONVERSATION_ID", "conv-1");
    let server = PermissionServer::new();

    // When the conversation's claude asks permission to run a Bash command
    let reply = server.decide_for_approval("Bash", &json!({"command": "rm -rf /tmp/x"}));

    // Then the conversation decided it, and the reply allows it
    let reply: Value = serde_json::from_str(&reply).expect("the reply is JSON");
    assert_eq!(reply["behavior"], "allow");
    assert_eq!(
        handler.calls(),
        vec![("conv-1".to_string(), "Bash".to_string())],
        "the handler must be asked about the tagged conversation and tool"
    );
}
