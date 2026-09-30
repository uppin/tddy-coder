//! `subagent_end` — finish a subagent conversation and hand its work to the caller.
//!
//! The counterpart of `subagent_cancel`: both close the conversation and delete its worktree, and
//! only this one first applies what the conversation committed to the caller's worktree as
//! uncommitted changes. In its own module because `server.rs` is over the file budget
//! (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`).

use crate::mcp_primitives::{schema_object, subagent_access_is_managed};
use crate::session_tool_client::{conversation_worktree, ConversationWorktreeOp};
use tddy_discovery::subagent_runtime::{subagent_error_json, subagent_sessions};

/// The tool as advertised.
pub(crate) fn subagent_end_tool_definition() -> rmcp::model::Tool {
    rmcp::model::Tool::new(
        "subagent_end",
        "Finish a subagent conversation and take its work. Everything the conversation's tool \
         calls changed — each committed in the conversation's own worktree, as the \
         `worktreeChange` of each tool result reported — is applied to YOUR worktree as \
         uncommitted changes, 3-way: where you changed the same lines since, the file gets \
         conflict markers and its path is listed in `pulled.conflicts`. Then the conversation's \
         worktree and branch are deleted and the conversation is closed. Returns {ended: true, \
         pulled: {files: {created, updated, removed}, lines: {added, removed}, conflicts}} — \
         `pulled` is null when the conversation never changed a file. Refused while a turn is \
         running: await it or cancel. Use subagent_cancel instead to discard the work.",
        schema_object(serde_json::json!({
            "type": "object",
            "required": ["sessionId"],
            "properties": {
                "sessionId": {"type": "string"}
            }
        })),
    )
}

/// `subagent_end { sessionId }`.
///
/// Pulls what the conversation committed into the caller's worktree, then closes it exactly as
/// `subagent_cancel` does — retire, then remove its worktree. A pull that fails leaves the
/// conversation open and its worktree in place, so the work is neither applied nor lost and the
/// caller can retry or cancel.
pub(crate) async fn subagent_end_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    let runs_here = match the_state_of(session_id).await {
        Ok(runs_here) => runs_here,
        Err(refusal) => return subagent_error_json(refusal),
    };
    // A conversation whose loop runs on the daemon never has a conversation worktree, and with no
    // daemon to ask (local access) none can exist either: nothing to pull.
    let pulled = match runs_here && subagent_access_is_managed() {
        true => match pull(session_id).await {
            Ok(pulled) => pulled,
            Err(reason) => return subagent_error_json(reason),
        },
        false => serde_json::Value::Null,
    };
    // The same close `subagent_cancel` performs, which also removes the worktree.
    crate::server::subagent_cancel_tool(serde_json::json!({ "sessionId": session_id })).await;
    serde_json::json!({ "ended": true, "pulled": pulled }).to_string()
}

/// Whether `session_id` names an open conversation with no turn outstanding, and if so whether its
/// loop runs in this process.
async fn the_state_of(session_id: &str) -> Result<bool, String> {
    let sessions = subagent_sessions().lock().await;
    let Some(conversation) = sessions.open.get(session_id) else {
        return Err(format!("unknown subagent session: {session_id}"));
    };
    match sessions.pending.queue_size(session_id) {
        0 => Ok(conversation.remote.is_none()),
        outstanding => Err(format!(
            "subagent session {session_id} has {outstanding} turn(s) still running; await them \
             or use subagent_cancel to discard the conversation"
        )),
    }
}

/// `ConversationWorktree{Pull}`: the `pulled` facts, or `null` when the conversation never wrote.
async fn pull(session_id: &str) -> Result<serde_json::Value, String> {
    let answer = conversation_worktree(session_id, ConversationWorktreeOp::Pull).await;
    let mut answer: serde_json::Value = serde_json::from_str(&answer)
        .map_err(|e| format!("the daemon's answer to the pull was not JSON ({e}): {answer}"))?;
    if answer.get("is_error") == Some(&serde_json::Value::Bool(true)) {
        return Err(format!(
            "could not pull subagent session {session_id}'s work: {}",
            answer["error"].as_str().unwrap_or("unknown error")
        ));
    }
    Ok(answer["pulled"].take())
}

/// Delete `session_id`'s worktree and branch on the daemon, after its conversation was retired.
///
/// A failure is logged, not raised: the conversation is already closed, and the answer the caller
/// is waiting on says so. The orphan it leaves — a worktree under `tmp/subagent-worktrees` nothing
/// will address again — is swept when the session worktree is.
// TODO(agent-worktree): sweep conversation worktrees whose conversation is gone (a crashed
// process, a failed remove) when a session starts, instead of leaving them to the session's end.
pub(crate) async fn discard_conversation_worktree(session_id: &str) {
    if !subagent_access_is_managed() {
        return;
    }
    let answer = conversation_worktree(session_id, ConversationWorktreeOp::Remove).await;
    let failed = serde_json::from_str::<serde_json::Value>(&answer)
        .map(|body| body.get("is_error") == Some(&serde_json::Value::Bool(true)))
        .unwrap_or(true);
    if failed {
        log::warn!(
            target: "tddy_tools::subagent_end",
            "could not remove subagent session {session_id}'s worktree: {answer}"
        );
    }
}
