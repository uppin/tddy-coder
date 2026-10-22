//! OpenCode CLI sandbox recipe — reads and policy for `opencode` running in the runner's PTY.

use std::path::Path;

use tddy_sandbox::builder::PolicySpec;
use tddy_sandbox::{process_exec_reads, ReadSpec};

/// Read grants for a sandboxed `opencode` binary (baseline + toolchain + binary deps).
pub fn process_opencode_exec_reads(opencode_binary: &Path) -> Vec<ReadSpec> {
    process_exec_reads(opencode_binary)
}

/// Policy for OpenCode's TUI. OpenCode runs on Bun, whose JavaScriptCore generates code at run
/// time, so it takes the same Node/V8 interactive policy as Claude Code.
pub fn opencode_interactive_policy() -> PolicySpec {
    crate::claude_cli::claude_interactive_policy()
}
