//! Restructure extension point: the `restructure_*` session tools.
//!
//! An agent restructures a session's worktree through six tool calls — `restructure_load`,
//! `restructure_check`, `restructure_apply`, `restructure_status`, `restructure_plans` and
//! `restructure_anchors` — relayed to the host over the session-tool transport exactly like the
//! `Lsp*` tools, and answered there against the warm index the daemon manages.
//!
//! This module is the port, mirroring [`super::lsp`]: the tool names, the env gate the host sets
//! when it can serve them, the [`RestructureExecutor`] trait and a process-global registry. The
//! executor itself lives with the index client (`tddy_lsp_executor::restructure_via_index`), so
//! neither `tddy-tool-engine` nor the in-jail `tddy-tools` takes on the restructure engine as a
//! dependency.

use std::path::Path;
use std::sync::{Arc, OnceLock};

/// Env var the host sets per session when it can serve the `restructure_*` tools (it manages a warm
/// index). Its presence (non-empty) gates whether the tools are exposed to the agent — the
/// `TDDY_LSP_TOOLS` precedent.
pub const RESTRUCTURE_TOOLS_ENV: &str = "TDDY_RESTRUCTURE_TOOLS";

/// The six restructure tool names, in catalog order.
pub const RESTRUCTURE_TOOL_NAMES: [&str; 6] = [
    "restructure_load",
    "restructure_check",
    "restructure_apply",
    "restructure_status",
    "restructure_plans",
    "restructure_anchors",
];

/// Whether the restructure tools should be exposed for this session (the `TDDY_RESTRUCTURE_TOOLS`
/// gate). A blank value counts as unset.
pub fn restructure_tools_enabled() -> bool {
    std::env::var(RESTRUCTURE_TOOLS_ENV).is_ok_and(|value| !value.trim().is_empty())
}

/// Whether `tool_name` is one of the six restructure tools.
pub fn is_restructure_tool(tool_name: &str) -> bool {
    RESTRUCTURE_TOOL_NAMES.contains(&tool_name)
}

/// Answers the `restructure_*` tools for a session's worktree.
///
/// `worktree` is the root the host resolved from the session — never a path the jail supplied. Every
/// plan or file path in `args` is resolved inside it, and refused when it leaves it, before the index
/// is asked. The answer is the tool's structured JSON result; an `Err` is a refusal the agent reads.
#[async_trait::async_trait]
pub trait RestructureExecutor: Send + Sync {
    async fn execute(
        &self,
        worktree: &Path,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

static REGISTERED: OnceLock<Arc<dyn RestructureExecutor>> = OnceLock::new();

/// Register the process-wide restructure executor. The first registration wins.
pub fn register_restructure_executor(executor: Arc<dyn RestructureExecutor>) {
    let _ = REGISTERED.set(executor);
}

/// The registered executor, if any.
pub fn restructure_executor() -> Option<Arc<dyn RestructureExecutor>> {
    REGISTERED.get().cloned()
}
