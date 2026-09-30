//! `subagent_pull` — take a chosen range of a subagent conversation's commits into your worktree,
//! while the conversation carries on. In its own module because `server.rs` is over the file budget
//! (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`).

use crate::mcp_primitives::schema_object;

/// The tool as advertised.
#[allow(dead_code)] // TODO(range-pull): routed in `subagent_tool_router`
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
#[allow(dead_code)] // TODO(range-pull): routed in `subagent_tool_router`
pub(crate) async fn subagent_pull_tool(args: serde_json::Value) -> String {
    // TODO(range-pull): implement — refuse while a turn is pending; `{"pulled": null}` without
    // asking the daemon when the conversation never reported a `worktreeChange`; otherwise
    // `tddy_session_tool_client::pull_conversation_range` with the conversation's `PullLedger`, and
    // record what it applied
    let _ = args;
    tddy_discovery::subagent_runtime::subagent_error_json("subagent_pull is not implemented yet")
}
