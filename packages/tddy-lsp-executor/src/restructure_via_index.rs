//! The `restructure_*` session tools, answered on the host by the warm index the daemon manages.
//!
//! The tools reach the host the way the `Lsp*` tools do — an `ExecuteTool` relayed over the
//! session-tool transport — and `tddy_tool_engine` hands them to the registered
//! [`RestructureExecutor`]. This is that executor: it dials the index through the same
//! [`IndexChannel`] port [`crate::index_backed::IndexLspExecutor`] dials, roots every request at the
//! session's worktree **as the host resolved it**, and binds every plan or file path the agent names
//! with [`crate::index_backed::bind_to_session_worktree`] — one binding path for both tool families, so a path from the
//! jail never names a root and never leaves the worktree.
//!
//! # Tool → index request
//!
//! | Tool | Arguments | Index RPC |
//! |---|---|---|
//! | `restructure_load` | `plans: [path]` | `LoadPlans` |
//! | `restructure_check` | `plan`, `deep?`, `file_budget?` | `Check` (streamed) |
//! | `restructure_apply` | `plan`, `dry_run?`, `resume?`, `from?`, `stop_after?` | `Apply` (streamed) |
//! | `restructure_status` | `plan` | `PlanStatus` |
//! | `restructure_plans` | — | `ListPlans` |
//! | `restructure_anchors` | `file`, `items?: [name]`, `at?: range` | `Anchors` |
//!
//! Every path is sent relative to the worktree; `workspace_root` is always the worktree itself.
//!
//! # Results
//!
//! `restructure_check` and `restructure_apply` answer one **run** object, folded from the event
//! stream (indexing progress is dropped):
//!
//! ```json
//! {
//!   "plan": "plans/split.jsonl",
//!   "findings":   [{ "operation": 1, "detail": "…" }],
//!   "operations": [{ "index": 0, "op_id": "op-1", "kind": "move_item",
//!                    "files": ["src/a.rs"], "visibility": [], "rehearsed_only": false }],
//!   "notes":      ["…"],
//!   "outcome":    { "applied": 1, "total": 2, "stopped_early": false },
//!   "refusal":    null
//! }
//! ```
//!
//! A run the index refused mid-stream still answers `Ok`: the operations it applied before the
//! refusal are on disk and the agent needs them. `outcome` is then `null` and `refusal` is
//! `{ "class", "message", "stale" }` — `class` the gRPC status class in snake case
//! (`failed_precondition`, `invalid_argument`, …, the classes `tddy_index_daemon::status` assigns),
//! and, for a `failed_precondition`, `stale` the plan's held stale operations as `PlanStatus`
//! reports them (`[{ "op", "reason" }]`), so a stale operation is refused **by its id**.
//!
//! `restructure_load` and `restructure_plans` answer `{ "plans": [{ "plan", "ops", "dirty",
//! "stale": [{ "op", "reason" }] }] }`; `restructure_status` answers `{ "completed", "in_flight",
//! "pending", "failed", "stale" }`; `restructure_anchors` answers `{ "range", "anchor_json" }` in
//! the index's one-based byte coordinates — the coordinates a plan's anchors are written in.
//!
//! An `Err` is a refusal before the index was asked (a path outside the worktree, a missing
//! argument) or a failure to reach it.

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tddy_core::toolcall::restructure::RestructureExecutor;

use crate::index_backed::IndexChannel;

/// Answers the `restructure_*` tools from the warm index reached through an [`IndexChannel`].
pub struct IndexRestructureExecutor {
    #[allow(dead_code)] // TODO(session-restructure-tools): dialled by every tool call.
    index: Arc<dyn IndexChannel>,
}

impl IndexRestructureExecutor {
    /// An executor asking the index `index` connects to.
    #[must_use]
    pub fn new(index: Arc<dyn IndexChannel>) -> Self {
        Self { index }
    }
}

/// Not served yet: every tool answers this until the index is asked.
fn not_served_yet(tool: &str) -> String {
    format!("{tool} through the warm index is not served yet — TODO(session-restructure-tools)")
}

#[async_trait::async_trait]
impl RestructureExecutor for IndexRestructureExecutor {
    async fn execute(
        &self,
        _worktree: &Path,
        tool_name: &str,
        _args: &Value,
    ) -> Result<Value, String> {
        // TODO(session-restructure-tools): per the module docs —
        //   1. bind `plan` / `plans[]` / `file` with `bind_to_session_worktree(worktree, …)`,
        //      refusing before the index is dialled;
        //   2. `self.index.connect()`, `CodeIndexServiceClient::new(channel)`;
        //   3. send the request rooted at `worktree`, fold a streamed `Check` / `Apply` into the
        //      run object (refusal class from the `Status` code; on `failed_precondition` ask
        //      `PlanStatus` for the stale operations), render the unary answers as documented.
        Err(not_served_yet(tool_name))
    }
}
