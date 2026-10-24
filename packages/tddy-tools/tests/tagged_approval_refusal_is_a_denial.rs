//! Acceptance: when a headless conversation's decision point refuses to decide a tool-use approval,
//! Claude receives a denial that carries the refusal, never an allow.
//!
//! `PermissionServer` reads `TDDY_SOCKET` and `TDDY_CONVERSATION_ID` when it is constructed, and both
//! are process-wide, so this file holds one test. The listener's socket path is keyed only by process
//! id, which is the other reason a file holds a single test.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tddy_core::toolcall::{
    start_toolcall_listener_with_handlers, ConversationApprovalHandler, ListenerHandlers,
};
use tddy_tools::server::PermissionServer;

/// Refuses every approval it is asked, the way a conversation with no answer to give would.
struct RefusingApprovalHandler;

#[async_trait]
impl ConversationApprovalHandler for RefusingApprovalHandler {
    async fn approve(
        &self,
        _conversation_id: &str,
        _tool_name: &str,
        _input: &Value,
    ) -> Result<bool, String> {
        Err("refused by the test".to_string())
    }
}

/// **tagged_approval_refusal_is_a_denial**: a refusal from the conversation's decision point reaches
/// Claude as a denial carrying the refusal's message.
#[test]
fn tagged_bash_approval_refused_by_its_conversation_is_denied_with_the_refusal() {
    // Given a listener whose conversation-approval handler refuses the request
    let data_dir = tempfile::tempdir().expect("temp dir");
    let handlers = ListenerHandlers {
        conversation_approval: Some(Arc::new(RefusingApprovalHandler)),
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

    // Then the reply denies it, and the denial carries the refusal
    let reply: Value = serde_json::from_str(&reply).expect("the reply is JSON");
    assert_eq!(reply["behavior"], "deny");
    assert_eq!(reply["message"], "refused by the test");
}
