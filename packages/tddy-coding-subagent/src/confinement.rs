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

/// The native tools a confined coding agent loses: Claude's own file-system and shell tools. The
/// same set a split session's main agent has withdrawn (`split_session::NATIVE_FILESYSTEM_TOOLS`,
/// which this crate cannot depend on).
const CLAUDE_NATIVE_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "MultiEdit",
    "NotebookEdit",
    "Bash",
    "BashOutput",
    "KillShell",
    "Glob",
    "Grep",
    "LS",
];

/// OpenCode's built-in file-system and shell tools, by the names its `tools` config uses.
const OPENCODE_NATIVE_TOOLS: &[&str] = &[
    "bash", "read", "write", "edit", "patch", "glob", "grep", "list",
];

/// The `_meta` a `session/new` of `harness` carries to withdraw its native tools; `None` when the
/// harness withdraws them another way (see [`withdrawal_env`]) or cannot.
///
/// `claude-acp`: `claude-agent-acp` merges `_meta.claudeCode.options` into the Claude Code SDK
/// options, whose `disallowedTools` removes a tool outright. DOCUMENTED, not verified against a live
/// adapter; that `_meta` reaches `session/new` is verified against the stub.
pub fn withdrawal_session_meta(
    harness: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    if harness != "claude-acp" {
        return None;
    }
    let mut meta = serde_json::Map::new();
    meta.insert(
        "claudeCode".to_string(),
        serde_json::json!({ "options": { "disallowedTools": CLAUDE_NATIVE_TOOLS } }),
    );
    Some(meta)
}

/// The environment that withdraws `harness`'s native tools; empty when it uses [`withdrawal_session_meta`]
/// or cannot.
///
/// `opencode`: `OPENCODE_CONFIG_CONTENT` is inline JSON config merged over the others, and its
/// `tools` table switches built-in tools off. DOCUMENTED, not verified against a live OpenCode.
pub fn withdrawal_env(harness: &str) -> Vec<(String, String)> {
    if harness != "opencode" {
        return Vec::new();
    }
    let tools: serde_json::Map<String, serde_json::Value> = OPENCODE_NATIVE_TOOLS
        .iter()
        .map(|tool| ((*tool).to_string(), serde_json::Value::Bool(false)))
        .collect();
    vec![(
        "OPENCODE_CONFIG_CONTENT".to_string(),
        serde_json::json!({ "tools": tools }).to_string(),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_withdraws_its_native_tools_through_session_meta() {
        let meta = withdrawal_session_meta("claude-acp").expect("meta");
        let tools = &meta["claudeCode"]["options"]["disallowedTools"];
        assert!(tools.as_array().expect("array").iter().any(|t| t == "Bash"));
        assert!(withdrawal_env("claude-acp").is_empty());
    }

    #[test]
    fn opencode_withdraws_its_native_tools_through_config_content() {
        let env = withdrawal_env("opencode");
        let config: serde_json::Value = serde_json::from_str(&env[0].1).expect("JSON");
        assert_eq!(env[0].0, "OPENCODE_CONFIG_CONTENT");
        assert_eq!(config["tools"]["bash"], false);
        assert!(withdrawal_session_meta("opencode").is_none());
    }

    #[test]
    fn a_harness_that_cannot_withdraw_gets_nothing() {
        assert!(withdrawal_session_meta("codex-acp").is_none());
        assert!(withdrawal_env("codex-acp").is_empty());
    }
}
