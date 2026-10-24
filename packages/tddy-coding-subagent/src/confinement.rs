//! Whether a harness can be run under a managed session's confinement.
//!
//! In a managed-codebase session the main agent has no native file-system or shell tools; a
//! subagent of that session must not have them either. Withdrawing them is harness-specific, and a
//! harness this table cannot prove withdrawable is refused in managed sessions — never launched
//! unconfined.
//!
//! The mechanism each `WithdrawsNativeTools` harness relies on (the launcher applies it):
//!
//! - `claude-acp` (`claude-agent-acp`): `session/new` `_meta.claudeCode.options.disallowedTools`
//!   names the native tools to withdraw (`Bash`, `Read`, `Edit`, `Write`, `Glob`, `Grep`, …); the
//!   MCP tools of `tddy-tools --mcp` remain.
//! - `opencode`: its config's `permission` / `tools` table (`OPENCODE_CONFIG_CONTENT`) disables the
//!   built-in `bash`, `read`, `edit`, `write`, `glob`, `grep` tools.
//!
//! `codex-acp` offers sandbox modes only, no tool withdrawal, so it is `CannotWithdraw`.

/// What a harness can do about its native tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confinement {
    /// Its native file-system and shell tools can be withdrawn, leaving only MCP tools.
    WithdrawsNativeTools,
    /// It cannot have them withdrawn; refused in managed sessions.
    CannotWithdraw,
}

/// The confinement `harness` supports. Anything not proven is [`Confinement::CannotWithdraw`].
pub fn confinement_for(harness: &str) -> Confinement {
    match harness {
        "claude-acp" | "opencode" => Confinement::WithdrawsNativeTools,
        _ => Confinement::CannotWithdraw,
    }
}
