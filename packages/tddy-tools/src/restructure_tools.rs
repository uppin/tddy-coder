//! The `restructure_*` MCP tools: restructure the session's worktree through tool calls on the warm
//! index, instead of shelling out to `tddy-tools restructure` (which needs `TDDY_INDEX_SOCKET`, set in
//! no session and unreachable from a jail).
//!
//! The six tools are catalog-driven: each def becomes a route of [`dynamic_tool_router`], so a call
//! is forwarded by name over the session-tool transport to the host's `ExecuteTool` — exactly the
//! path the `Lsp*` tools take — where `tddy_tool_engine` hands it to the registered
//! `tddy_lsp_executor::restructure_via_index::IndexRestructureExecutor`. Nothing here executes a
//! plan: the host binds the paths to the session's worktree and asks the index. The result shapes
//! are documented on that executor.
//!
//! Merged into the server only when the host set `TDDY_RESTRUCTURE_TOOLS`
//! ([`tddy_core::toolcall::restructure::restructure_tools_enabled`]) — the `TDDY_LSP_TOOLS`
//! precedent — so a session whose host manages no index is never offered a tool nothing answers.
//!
//! Lives in its own module rather than `server.rs`, which is over its size budget.

use rmcp::handler::server::router::tool::ToolRouter;
use tddy_core::toolcall::restructure::RESTRUCTURE_TOOL_NAMES;

use crate::mcp_primitives::RemoteToolDef;
use crate::server::{dynamic_tool_router, PermissionServer};

const PLAN_SCHEMA: &str = r#"{"type":"object","required":["plan"],"properties":{"plan":{"type":"string","description":"JSONL plan path inside the session's worktree, relative to it or absolute"}}}"#;

const LOAD_SCHEMA: &str = r#"{"type":"object","required":["plans"],"properties":{"plans":{"type":"array","items":{"type":"string"},"description":"JSONL plan paths inside the session's worktree, relative to it or absolute"}}}"#;

const CHECK_SCHEMA: &str = r#"{"type":"object","required":["plan"],"properties":{"plan":{"type":"string","description":"JSONL plan path inside the session's worktree, relative to it or absolute"},"deep":{"type":"boolean","description":"Resolve each operation through the language server as well as reading the text"},"file_budget":{"type":"integer","minimum":0,"description":"Line count above which a file the plan's anchors name is reported; 0 reports none"}}}"#;

const APPLY_SCHEMA: &str = r#"{"type":"object","required":["plan"],"properties":{"plan":{"type":"string","description":"JSONL plan path inside the session's worktree, relative to it or absolute"},"dry_run":{"type":"boolean","description":"Rehearse in an overlay; nothing is written"},"resume":{"type":"boolean","description":"Continue the plan's existing journal rather than refusing it"},"from":{"type":"integer","minimum":0,"description":"Start at this operation index instead of the journal's next"},"stop_after":{"type":"integer","minimum":0,"description":"Stop after this operation index"}}}"#;

const PLANS_SCHEMA: &str = r#"{"type":"object","properties":{}}"#;

const ANCHORS_SCHEMA: &str = r#"{"type":"object","required":["file"],"properties":{"file":{"type":"string","description":"Source file inside the session's worktree, relative to it or absolute"},"items":{"type":"array","items":{"type":"string"},"description":"Named run of items to anchor, trivia included"},"at":{"type":"object","description":"Anchor the innermost item enclosing this one-based byte range instead of named items","required":["start","end"],"properties":{"start":{"type":"object","required":["line","column"],"properties":{"line":{"type":"integer","minimum":1},"column":{"type":"integer","minimum":1}}},"end":{"type":"object","required":["line","column"],"properties":{"line":{"type":"integer","minimum":1},"column":{"type":"integer","minimum":1}}}}}}}"#;

/// The six restructure tools in the MCP shape, in [`RESTRUCTURE_TOOL_NAMES`] order.
pub fn restructure_tool_defs() -> Vec<RemoteToolDef> {
    let [load, check, apply, status, plans, anchors] = RESTRUCTURE_TOOL_NAMES;
    [
        (
            load,
            "Load restructure plans into the warm index's plan store for the session's worktree; \
             answers each held plan with its operation count and stale operations.",
            LOAD_SCHEMA,
        ),
        (
            check,
            "Check a restructure plan against the session's worktree without writing anything; \
             answers its findings as JSON.",
            CHECK_SCHEMA,
        ),
        (
            apply,
            "Apply a restructure plan to the session's worktree through the warm index; answers \
             per-operation outcomes, and a refusal naming any stale operation by id.",
            APPLY_SCHEMA,
        ),
        (
            status,
            "How far a restructure plan's journal got: completed, in flight, pending, failed, and \
             its stale operations.",
            PLAN_SCHEMA,
        ),
        (
            plans,
            "The restructure plans the warm index holds for the session's worktree.",
            PLANS_SCHEMA,
        ),
        (
            anchors,
            "A range anchor covering a named run of items (or the item enclosing a range) in a file \
             of the session's worktree, as the JSON fragment a plan carries.",
            ANCHORS_SCHEMA,
        ),
    ]
    .into_iter()
    .map(|(name, description, schema)| RemoteToolDef {
        name: name.to_string(),
        description: description.to_string(),
        input_schema_json: schema.to_string(),
    })
    .collect()
}

/// The router [`PermissionServer::new`] merges when the host set `TDDY_RESTRUCTURE_TOOLS`.
pub fn restructure_tool_router() -> ToolRouter<PermissionServer> {
    dynamic_tool_router(&restructure_tool_defs())
}
