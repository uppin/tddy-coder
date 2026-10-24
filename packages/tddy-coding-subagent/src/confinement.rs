//! Whether a harness can be run under a managed session's confinement.
//!
//! In a managed-codebase session the main agent has no native file-system or shell tools; a
//! subagent of that session must not have them either. Withdrawing them is harness-specific, and a
//! harness this table cannot prove withdrawable is refused in managed sessions — never launched
//! unconfined.

/// What a harness can do about its native tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confinement {
    /// Its native file-system and shell tools can be withdrawn, leaving only MCP tools.
    WithdrawsNativeTools,
    /// It cannot have them withdrawn; refused in managed sessions.
    CannotWithdraw,
}

/// The confinement `harness` supports.
pub fn confinement_for(_harness: &str) -> Confinement {
    // TODO(acp-subagent): implement
    Confinement::CannotWithdraw
}
