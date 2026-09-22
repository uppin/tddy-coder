//! What a recipe tells a backend about one goal: the permission it runs under and its CLI hints.
//!
//! Plain data the backend reads and the workflow recipe produces, so it lives in the vocabulary crate
//! both name rather than inside either of them.

/// Backend-agnostic permission hint (mapped per backend in claude/cursor/acp).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionHint {
    ReadOnly,
    AcceptEdits,
}

/// Per-goal configuration for backends (replaces matching on a `Goal` enum).
#[derive(Debug, Clone)]
pub struct GoalHints {
    pub display_name: String,
    pub permission: PermissionHint,
    pub allowed_tools: Vec<String>,
    pub default_model: Option<String>,
    pub agent_output: bool,
    /// When true, backends enable vendor “plan mode” CLI flags (e.g. Cursor `--plan`, Claude `--permission-mode plan`).
    pub agent_cli_plan_mode: bool,
    /// Claude CLI: if the process exits non-zero but stdout contains `<structured-response`, treat as success.
    /// Set by the recipe for goals that emit structured JSON despite a non-zero exit code.
    pub claude_nonzero_exit_ok_if_structured_response: bool,
}
