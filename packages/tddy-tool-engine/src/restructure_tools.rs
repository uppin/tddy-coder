//! Dispatch of the six `restructure_*` session tools to the registered
//! [`tddy_core::toolcall::restructure::RestructureExecutor`].
//!
//! The executor — `tddy_lsp_executor::restructure_via_index::IndexRestructureExecutor` on a host
//! that manages a warm index — binds every path to `worktree_root` and asks the index; this module
//! only hands it the host-resolved root and wraps its answer. A host without an executor never
//! advertises the tools (`TDDY_RESTRUCTURE_TOOLS` unset), so a call reaching here without one is a
//! refusal, not a fallback.

use std::path::Path;

use crate::ToolOutcome;

pub(crate) async fn tool_restructure(
    worktree_root: &Path,
    tool_name: &str,
    args: &serde_json::Value,
) -> ToolOutcome {
    let Some(executor) = tddy_core::toolcall::restructure::restructure_executor() else {
        return ToolOutcome::err(format!("{tool_name}: no warm index available"));
    };
    match executor.execute(worktree_root, tool_name, args).await {
        Ok(value) => ToolOutcome::ok(value.to_string()),
        Err(e) => ToolOutcome::err(format!("{tool_name}: {e}")),
    }
}
