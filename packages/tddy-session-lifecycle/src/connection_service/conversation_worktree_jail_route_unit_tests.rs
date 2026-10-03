//! A subagent conversation's call on a sandboxed workspace session, as the jail receives it.
//!
//! The jail finds a conversation's root itself, from the `conversation_id` it is sent, and the
//! conversation's worktree exists only from its first write. So a call that is told to run in the
//! conversation before that write would run in a directory that is not there. The root the
//! conversation rule chose has to decide what the jail is sent, and this is the seam that holds
//! both sides of that: `LocalExecTools::run_exec_tool_locally`, with a jail that records the
//! request it is handed. (The runner's own side — `conversation_root::tool_root` — is covered by
//! `tddy-sandbox-runner`; a real jail is not started here, so this proves what reaches it rather
//! than what the OS sandbox then does.)
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md

use super::workspace_sandbox_roster_dispatch_unit_tests::{
    a_sandboxed_workspace_session, SeededWorkspace,
};
use super::*;

const CONVERSATION: &str = "explore";

impl SeededWorkspace {
    /// One call of the `explore` conversation, as an `ExecuteTool` request would carry it.
    async fn conversation_calls(&self, tool: &str, args: serde_json::Value) {
        let request = ExecuteToolRequest {
            session_id: self.session_id.clone(),
            conversation_id: CONVERSATION.to_string(),
            tool_name: tool.to_string(),
            args_json: args.to_string(),
            ..Default::default()
        };
        let answered = self
            .service
            .run_exec_tool_locally(&request, self.sessions_base(), &self.worktree)
            .await;
        assert!(
            !answered.is_error,
            "the call must run: {}",
            answered.error_message
        );
    }
}

#[tokio::test]
async fn a_read_before_the_conversations_first_write_is_sent_to_the_jail_for_the_session_root() {
    // Given
    let workspace = a_sandboxed_workspace_session(true).await;

    // When
    workspace
        .conversation_calls("Read", serde_json::json!({ "path": "README.md" }))
        .await;

    // Then
    assert_eq!(workspace.sandbox.conversations(), vec![String::new()]);
}

#[tokio::test]
async fn a_call_after_the_conversations_first_write_is_sent_to_the_jail_for_its_worktree() {
    // Given
    let workspace = a_sandboxed_workspace_session(true).await;
    workspace
        .conversation_calls(
            "Write",
            serde_json::json!({ "path": "new.rs", "contents": "pub fn new() {}\n" }),
        )
        .await;

    // When
    workspace
        .conversation_calls("Read", serde_json::json!({ "path": "new.rs" }))
        .await;

    // Then
    assert_eq!(
        workspace.sandbox.conversations(),
        vec![CONVERSATION.to_string(), CONVERSATION.to_string()]
    );
}
