//! `subagent_pull` — take a chosen range of a subagent conversation's commits into your worktree,
//! while the conversation carries on. In its own module because `server.rs` is over the file budget
//! (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`).

use crate::mcp_primitives::{schema_object, subagent_access_is_managed};
use crate::session_tool_client::pull_conversation_range;
use tddy_discovery::subagent_runtime::subagent_error_json;

/// The tool as advertised.
pub(crate) fn subagent_pull_tool_definition() -> rmcp::model::Tool {
    rmcp::model::Tool::new(
        "subagent_pull",
        "Take some of a subagent conversation's work now, without ending it. Applies the \
         conversation's commits `from` through `to` — BOTH inclusive, the `worktreeChange.commit` \
         short hashes its tool results reported — to YOUR worktree as uncommitted changes, one \
         commit at a time, 3-way: where you changed the same lines, the file gets conflict markers \
         and is listed in `conflicts`. Omit `from` for the earliest commit not yet pulled, `to` for \
         the latest. A commit already pulled is skipped, never applied twice, and a later \
         subagent_end takes only what is left. Returns {pulled: {commits, skipped, files: \
         {created, updated, removed}, lines: {added, removed}, conflicts}} — `pulled` is null when \
         the conversation never changed a file. Refused while a turn is running.",
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

/// `subagent_pull { sessionId, from?, to? }`.
///
/// Refused while a turn is outstanding, and for an unknown conversation. A conversation whose loop
/// runs on a daemon, and one with no daemon to ask, never has a conversation worktree: `pulled` is
/// null without asking. Otherwise the daemon applies the range, skipping what this conversation's
/// ledger holds, and what it applied is recorded.
pub(crate) async fn subagent_pull_tool(args: serde_json::Value) -> String {
    let Some(session_id) = args.get("sessionId").and_then(|v| v.as_str()) else {
        return subagent_error_json("missing required field: sessionId");
    };
    match pull_range(session_id, bound(&args, "from"), bound(&args, "to")).await {
        Ok(pulled) => serde_json::json!({ "pulled": pulled }).to_string(),
        Err(reason) => subagent_error_json(reason),
    }
}

/// The string argument `key` of `args`, if given.
pub(crate) fn bound<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

/// Apply `session_id`'s commits `from` through `to` that its ledger does not hold to the caller's
/// worktree, record them, and answer the `pulled` facts — `null` when the conversation never wrote.
/// An open conversation with a turn outstanding is refused.
pub(crate) async fn pull_range(
    session_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<serde_json::Value, String> {
    let runs_here = crate::subagent_end::the_state_of(session_id).await?;
    if !(runs_here && subagent_access_is_managed()) {
        return Ok(serde_json::Value::Null);
    }
    let already_pulled = crate::pull_ledger::pulled_by(session_id);
    let answer = pull_conversation_range(session_id, from, to, &already_pulled).await;
    let mut answer: serde_json::Value = serde_json::from_str(&answer)
        .map_err(|e| format!("the daemon's answer to the pull was not JSON ({e}): {answer}"))?;
    if answer.get("is_error") == Some(&serde_json::Value::Bool(true)) {
        return Err(format!(
            "could not pull subagent session {session_id}'s work: {}",
            answer["error"].as_str().unwrap_or("unknown error")
        ));
    }
    let pulled = answer["pulled"].take();
    let applied: Vec<String> = pulled["commits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|commit| commit.as_str().map(str::to_string))
        .collect();
    crate::pull_ledger::record_pulled(session_id, &applied);
    Ok(pulled)
}
