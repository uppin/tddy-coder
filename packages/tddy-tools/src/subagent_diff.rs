//! `subagent_diff` — read what a subagent conversation changed between two of its commits.
//!
//! Read-only, so it is allowed while a turn runs. In its own module because `server.rs` is over the
//! file budget (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`).

use crate::mcp_primitives::schema_object;

/// The tool as advertised.
#[allow(dead_code)] // TODO(diff): routed in `subagent_tool_router`
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
         Read-only: allowed while a turn is running.",
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
#[allow(dead_code)] // TODO(diff): routed in `subagent_tool_router`
pub(crate) async fn subagent_diff_tool(args: serde_json::Value) -> String {
    // TODO(diff): implement — refuse an unknown conversation and one that never reported a
    // `worktreeChange` without asking the daemon; otherwise
    // `tddy_session_tool_client::diff_conversation_worktree` and answer its `diff` object
    let _ = args;
    tddy_discovery::subagent_runtime::subagent_error_json("subagent_diff is not implemented yet")
}
