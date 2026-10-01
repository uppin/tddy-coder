//! A subagent conversation's tool call arriving over the jail's session channel.
//!
//! That channel reaches [`DaemonToolHandler`](crate::sandbox_session) directly and never enters the
//! exec-tool route, so it applies the conversation rule — the same
//! [`run_in_conversation`] the route uses — itself.

use std::path::Path;

use tddy_subagent_worktree::{run_in_conversation, ConversationId, ConversationWorktrees};
use tddy_task::TaskRegistry;
use tddy_tool_engine::{execute_tool_with_env, ToolOutcome};

/// One tool call as the jail's session channel delivers it.
pub(crate) struct ToolCall<'a> {
    /// The session worktree the call's session owns.
    pub worktree: &'a Path,
    /// The subagent conversation the call belongs to; empty for the session's own call.
    pub conversation_id: &'a str,
    pub tool_name: &'a str,
    pub args_json: &'a str,
    pub registry: &'a TaskRegistry,
    pub session_id: &'a str,
    pub env: &'a [(String, String)],
}

/// Run `call` — in the conversation's worktree under the session worktree when it names a
/// conversation, and put what it changed on the result; in the session worktree itself when it
/// does not. An id that cannot be a conversation, or a worktree that cannot be prepared, answers as
/// a tool error: the call never runs in the session worktree instead.
pub(crate) async fn execute_in_conversation(call: ToolCall<'_>) -> ToolOutcome {
    let ToolCall {
        worktree,
        conversation_id,
        tool_name,
        args_json,
        registry,
        session_id,
        env,
    } = call;
    if conversation_id.is_empty() {
        return execute_tool_with_env(worktree, tool_name, args_json, registry, session_id, env)
            .await;
    }
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
        Ok(run) => {
            let result_json = run.merge(&run.output.result_json);
            ToolOutcome {
                result_json,
                ..run.output
            }
        }
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
