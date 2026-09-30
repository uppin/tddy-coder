//! A subagent conversation's tool call arriving over the jail's session channel.
//!
//! That channel reaches [`DaemonToolHandler`](crate::sandbox_session) directly and never enters the
//! exec-tool route, so it applies the conversation rule — the same
//! [`run_in_conversation`] the route uses — itself.

use std::path::Path;

use tddy_subagent_worktree::{
    run_in_conversation, with_worktree_change, ConversationId, ConversationRun,
    ConversationWorktrees,
};
use tddy_task::TaskRegistry;
use tddy_tool_engine::{execute_tool_with_env, ToolOutcome};

/// Run `tool_name` for `conversation_id` in that conversation's worktree under `worktree`, and put
/// what it changed on the result. An id that cannot be a conversation, or a worktree that cannot be
/// prepared, answers as a tool error: the call never runs in the session worktree instead.
pub(crate) async fn execute_in_conversation(
    worktree: &Path,
    conversation_id: &str,
    tool_name: &str,
    args_json: &str,
    registry: &TaskRegistry,
    session_id: &str,
    env: &[(String, String)],
) -> ToolOutcome {
    let conversation = match ConversationId::parse(conversation_id) {
        Ok(conversation) => conversation,
        Err(unsafe_id) => return refused(unsafe_id.to_string()),
    };
    let worktrees = ConversationWorktrees::new(worktree, session_id);
    let run = run_in_conversation(&worktrees, &conversation, tool_name, |root| async move {
        execute_tool_with_env(&root, tool_name, args_json, registry, session_id, env).await
    })
    .await;
    match run {
        Ok(ConversationRun {
            mut output,
            change: Some(change),
        }) => {
            output.result_json = with_worktree_change(&output.result_json, &change);
            output
        }
        Ok(ConversationRun {
            output,
            change: None,
        }) => output,
        Err(e) => refused(format!(
            "conversation {conversation}: its worktree could not be prepared ({e}); refusing to \
             run the call in the session worktree instead"
        )),
    }
}

fn refused(message: String) -> ToolOutcome {
    log::warn!(target: "tddy_daemon_sandbox::conversation_tool", "{message}");
    ToolOutcome {
        result_json: serde_json::json!({ "error": message }).to_string(),
        is_error: true,
        error_message: message,
        job_id: String::new(),
        job_running: false,
    }
}
