//! `subagent_end` — finish a subagent conversation and hand its work to the caller.
//!
//! The counterpart of `subagent_cancel`: both close the conversation and delete its worktree, and
//! only this one first applies what the conversation committed to the caller's worktree as
//! uncommitted changes. In its own module because `server.rs` is over the file budget
//! (`packages/tddy-tools/docs/code-issues/oversized-file-server.md`).

use crate::mcp_primitives::schema_object;

/// The tool as advertised.
#[allow(dead_code)] // TODO(isolated-edits): routed in `subagent_tool_router`
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
#[allow(dead_code)] // TODO(isolated-edits): routed in `subagent_tool_router`
pub(crate) async fn subagent_end_tool(args: serde_json::Value) -> String {
    // TODO(isolated-edits): implement — refuse while a turn is pending; skip the daemon when the
    // conversation never reported a `worktreeChange`; otherwise `ConversationWorktree{Pull}` then
    // `{Remove}`; retire the conversation as `subagent_cancel` does
    let _ = args;
    tddy_discovery::subagent_runtime::subagent_error_json("subagent_end is not implemented yet")
}
