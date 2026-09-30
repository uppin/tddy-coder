//! A subagent conversation's calls to its facilitating daemon: its tool calls, which run in the
//! conversation's own worktree, and the worktree operations that are not tool calls.
//!
//! In its own module rather than beside [`crate::dispatch_session_tool`]: `lib.rs` is over the file
//! budget (`packages/tddy-session-tool-client/docs/code-issues/oversized-file-lib.md`), and this is
//! the one place the conversation id enters the envelope.

use tddy_service::proto::exec_tools::{
    conversation_worktree_request::Op, ConversationWorktreeRequest, ExecuteToolRequest, PullOp,
    RemoveOp,
};

use crate::SessionToolEnvelope;

/// A worktree operation a conversation asks its daemon for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationWorktreeOp {
    /// Hand everything committed since the base to the session worktree, 3-way.
    Pull,
    /// Delete the worktree, its branch and its base ref.
    Remove,
}

/// Run `tool_name` for `conversation_id` over whichever transport this process was given — the
/// same transports as [`crate::dispatch_session_tool`], with the conversation named on the wire.
pub async fn dispatch_conversation_tool(
    conversation_id: &str,
    tool_name: &str,
    args: serde_json::Value,
) -> String {
    // TODO(isolated-edits): implement over the four transports
    let _ = (conversation_id, tool_name, args);
    todo!("dispatch_conversation_tool")
}

/// Ask the facilitating daemon for `op` on `conversation_id`'s worktree; the answer's
/// `result_json`, or a `{"error", "is_error": true}` body naming why it could not be asked.
pub async fn conversation_worktree(conversation_id: &str, op: ConversationWorktreeOp) -> String {
    // TODO(isolated-edits): implement over the four transports
    let _ = (conversation_id, op);
    todo!("conversation_worktree")
}

/// The `ExecuteTool` a conversation's call sends.
#[allow(dead_code)] // TODO(isolated-edits): called by `dispatch_conversation_tool`
pub(crate) fn conversation_tool_request(
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
) -> ExecuteToolRequest {
    // TODO(isolated-edits): implement
    let _ = (envelope, conversation_id, tool_name, args);
    todo!("conversation_tool_request")
}

/// The `ConversationWorktree` request for `op`.
#[allow(dead_code)] // TODO(isolated-edits): called by `conversation_worktree`
pub(crate) fn conversation_worktree_request(
    envelope: &SessionToolEnvelope,
    conversation_id: &str,
    op: ConversationWorktreeOp,
) -> ConversationWorktreeRequest {
    // TODO(isolated-edits): implement
    let _ = (
        envelope,
        conversation_id,
        op,
        Op::Pull(PullOp {}),
        RemoveOp {},
    );
    todo!("conversation_worktree_request")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn an_envelope() -> SessionToolEnvelope {
        SessionToolEnvelope {
            session_id: "sess-1".into(),
            session_token: "token-1".into(),
            daemon_instance_id: "daemon-a".into(),
        }
    }

    #[test]
    fn a_conversation_tool_call_names_its_conversation_on_the_wire() {
        // Given
        let args = serde_json::json!({ "path": "a.txt", "contents": "a" });

        // When
        let request = conversation_tool_request(&an_envelope(), "explore", "Write", &args);

        // Then
        assert_eq!(
            request,
            ExecuteToolRequest {
                session_token: "token-1".into(),
                session_id: "sess-1".into(),
                tool_name: "Write".into(),
                args_json: args.to_string(),
                daemon_instance_id: "daemon-a".into(),
                conversation_id: "explore".into(),
            }
        );
    }

    #[test]
    fn a_pull_names_its_conversation_and_asks_for_a_pull() {
        assert_eq!(
            conversation_worktree_request(&an_envelope(), "explore", ConversationWorktreeOp::Pull),
            ConversationWorktreeRequest {
                session_token: "token-1".into(),
                session_id: "sess-1".into(),
                daemon_instance_id: "daemon-a".into(),
                conversation_id: "explore".into(),
                op: Some(Op::Pull(PullOp {})),
            }
        );
    }

    #[test]
    fn a_remove_asks_for_a_remove() {
        assert_eq!(
            conversation_worktree_request(
                &an_envelope(),
                "explore",
                ConversationWorktreeOp::Remove
            )
            .op,
            Some(Op::Remove(RemoveOp {}))
        );
    }
}
