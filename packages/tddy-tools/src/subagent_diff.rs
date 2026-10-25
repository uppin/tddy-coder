//! `subagent_diff` — read what a subagent conversation changed between two of its commits.
//!
//! Read-only, so it is allowed while a turn runs. In its own module because `server.rs` is over the
//! file budget (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`).

use crate::mcp_primitives::{schema_object, subagent_access_is_managed};
use crate::subagent_end::host_of;
use crate::worktree_route::{has_its_own_worktree, ConversationHost};
use tddy_discovery::subagent_runtime::subagent_error_json;
use tddy_session_tool_client::diff_conversation_worktree;

/// The tool as advertised.
pub(crate) fn subagent_diff_tool_definition() -> rmcp::model::Tool {
    rmcp::model::Tool::new(
        "subagent_diff",
        "Show what a subagent conversation changed between two of its commits — the \
         `worktreeChange.commit` short hashes its tool results reported. The diff is `from..to`, \
         as git means it: the changes made AFTER `from`, up to and including `to`. Omit `from` for \
         the conversation's starting point, `to` for its latest commit. Returns {from, to, \
         files: {created, updated, removed}, lines: {added, removed}, diff, truncated}; the diff \
         text is capped at 64 KiB (`truncated: true`), the counts always cover the whole range. A \
         commit that is not in the conversation — including one a rewind dropped — is refused. \
         A `shared` agent edits the session's own worktree, so its conversation has no worktree of its \
         own to diff: refused. Read-only: allowed while a turn is running.",
        schema_object(serde_json::json!({
            "type": "object",
            "required": ["sessionId"],
            "properties": {
                "sessionId": {"type": "string"},
                "from": {"type": "string", "description": "Exclusive lower bound; default: the base."},
                "to": {"type": "string", "description": "Inclusive upper bound; default: the tip."}
            }
        })),
    )
}

/// `subagent_diff { sessionId, from?, to? }`.
///
/// An unknown conversation, and one with no worktree of its own (see
/// [`has_its_own_worktree`]), are refused without asking. Otherwise the daemon answers: a conversation
/// that never wrote has no worktree, and a bound it does not have is refused, each by name. Takes no
/// lock on the conversation, so it reads while a turn runs.
pub(crate) async fn subagent_diff_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    let bound = |key: &str| args.get(key).and_then(|v| v.as_str());
    let host = match host_of(session_id).await {
        Ok(host) => host,
        Err(refusal) => return subagent_error_json(refusal),
    };
    if !has_its_own_worktree(host) {
        return subagent_error_json(no_worktree_reason(session_id, host));
    }
    if !subagent_access_is_managed() {
        return subagent_error_json(format!(
            "subagent session {session_id} has no worktree: without a daemon its tools cannot write"
        ));
    }
    let answer = diff_conversation_worktree(session_id, bound("from"), bound("to")).await;
    let mut answer: serde_json::Value = match serde_json::from_str(&answer) {
        Ok(answer) => answer,
        Err(e) => {
            return subagent_error_json(format!(
                "the daemon's answer to the diff was not JSON ({e}): {answer}"
            ))
        }
    };
    if answer.get("is_error") == Some(&serde_json::Value::Bool(true)) {
        return answer.to_string();
    }
    answer["diff"].take().to_string()
}

/// Why `session_id`, whose turns run at `host`, has no worktree of its own to diff.
fn no_worktree_reason(session_id: &str, host: ConversationHost) -> String {
    match host {
        ConversationHost::InProcess { .. } => format!(
            "subagent session {session_id} has no worktree: without a daemon its tools cannot write"
        ),
        ConversationHost::Daemon { .. } => format!(
            "subagent session {session_id} runs on a daemon and its agent is shared: it wrote the \
             session's worktree directly, so it has no worktree of its own to diff"
        ),
    }
}
