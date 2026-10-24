//! `subagent_end` — finish a subagent conversation and hand its work to the caller.
//!
//! The counterpart of `subagent_cancel`: both close the conversation and delete its worktree, and
//! only this one first applies what the conversation committed to the caller's worktree as
//! uncommitted changes. In its own module because `server.rs` is over the file budget
//! (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`).

use crate::mcp_primitives::{schema_object, subagent_access_is_managed};
use crate::session_tool_client::{conversation_worktree, ConversationWorktreeOp};
use crate::subagent_pull::{bound, pull_range};
use crate::worktree_route::ConversationHost;
use tddy_discovery::subagent_runtime::{subagent_error_json, subagent_sessions};

/// The tool as advertised.
pub(crate) fn subagent_end_tool_definition() -> rmcp::model::Tool {
    rmcp::model::Tool::new(
        "subagent_end",
        "Finish a subagent conversation and take its work. Everything the conversation's tool \
         calls changed — each committed in the conversation's own worktree, as the \
         `worktreeChange` of each tool result reported — is applied to YOUR worktree as \
         uncommitted changes, 3-way: where you changed the same lines since, the file gets \
         conflict markers and its path is listed in `pulled.conflicts`. `from` and `to` choose a range \
         instead of everything — BOTH inclusive, the `worktreeChange.commit` short hashes the tool \
         results reported; commits already taken by subagent_pull are skipped, never applied \
         twice. Then the conversation's \
         worktree and branch are deleted and the conversation is closed. Returns {ended: true, \
         pulled: {commits, skipped, files: {created, updated, removed}, lines: {added, \
         removed}, conflicts}} — `pulled` is null when the conversation never changed a file, and for a `shared` agent, which edits the session's own \
         worktree and has nothing of its own to take. Refused while a turn is \
         running: await it or cancel. Use subagent_cancel instead to discard the work.",
        schema_object(serde_json::json!({
            "type": "object",
            "required": ["sessionId"],
            "properties": {
                "sessionId": {"type": "string"},
                "from": {"type": "string", "description": "First commit to take (inclusive); default: the earliest not yet pulled."},
                "to": {"type": "string", "description": "Last commit to take (inclusive); default: the latest."}
            }
        })),
    )
}

/// `subagent_end { sessionId, from?, to? }`.
///
/// Pulls what the conversation committed (the range, less what was already pulled) into the
/// caller's worktree, then closes it exactly as `subagent_cancel` does — retire, then remove its
/// worktree. A pull that fails leaves the conversation open and its worktree in place, so the work
/// is neither applied nor lost and the caller can retry or cancel.
pub(crate) async fn subagent_end_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    // A conversation with no worktree of its own — a shared agent's, or one with no daemon to ask
    // (local access) — has nothing to take: `pull_range` answers null without asking.
    let pulled = match pull_range(session_id, bound(&args, "from"), bound(&args, "to")).await {
        Ok(pulled) => pulled,
        Err(reason) => return subagent_error_json(reason),
    };
    // The same close `subagent_cancel` performs, which also removes the worktree.
    crate::server::subagent_cancel_tool(serde_json::json!({ "sessionId": session_id })).await;
    serde_json::json!({ "ended": true, "pulled": pulled }).to_string()
}

/// Where `session_id`'s turns run: in this process, or on the daemon — and for the daemon, whether
/// its agent is `clone`-placed. An unknown conversation is refused.
pub(crate) async fn host_of(session_id: &str) -> Result<ConversationHost, String> {
    let sessions = subagent_sessions().lock().await;
    host_in(&sessions, session_id)
}

fn host_in(
    sessions: &tddy_discovery::subagent_runtime::SubagentConversations,
    session_id: &str,
) -> Result<ConversationHost, String> {
    let conversation = sessions
        .open
        .get(session_id)
        .ok_or_else(|| format!("unknown subagent session: {session_id}"))?;
    Ok(match conversation.remote {
        None => ConversationHost::InProcess {
            managed_access: subagent_access_is_managed(),
        },
        Some(_) => ConversationHost::Daemon {
            clone: conversation.clone_placed,
        },
    })
}

/// [`host_of`] for a conversation with no turn outstanding: what `subagent_pull` and `subagent_end`
/// take is what the turns have committed, so they wait for the turns to end. Both are read under
/// one lock, so the answer is about one state of the conversation.
pub(crate) async fn the_host_of(session_id: &str) -> Result<ConversationHost, String> {
    let sessions = subagent_sessions().lock().await;
    let host = host_in(&sessions, session_id)?;
    let outstanding = sessions.pending.queue_size(session_id);
    match outstanding {
        0 => Ok(host),
        outstanding => Err(format!(
            "subagent session {session_id} has {outstanding} turn(s) still running; await them \
             or use subagent_cancel to discard the conversation"
        )),
    }
}

/// Delete `session_id`'s worktree and branch on the daemon, after its conversation was retired.
///
/// A failure is logged, not raised: the conversation is already closed, and the answer the caller
/// is waiting on says so. The orphan it leaves — a worktree under `tmp/subagent-worktrees` nothing
/// will address again — is swept when the session worktree is.
// TODO(agent-worktree): sweep conversation worktrees whose conversation is gone (a crashed
// process, a failed remove) when a session starts, instead of leaving them to the session's end.
pub(crate) async fn discard_conversation_worktree(session_id: &str) {
    crate::pull_ledger::forget_conversation(session_id);
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
