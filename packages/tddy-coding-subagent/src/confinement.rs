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
//! - `opencode`: its config's `tools` table, handed over inline as [`OPENCODE_CONFIG_CONTENT`]
//!   ([`opencode_withdrawal_config`]), disables the built-in `bash`, `read`, `edit`, `write`, `glob`,
//!   `grep` tools.
//!
//! `codex-acp` offers sandbox modes only, no tool withdrawal, so it is `CannotWithdraw`.

use tddy_coding_agent_specs::CodingAgentSpec;

use crate::catalog::{ACP_HARNESSES, CLAUDE_ACP, OPENCODE};

/// The variable `opencode` reads inline JSON configuration from, merged over its other config.
pub const OPENCODE_CONFIG_CONTENT: &str = "OPENCODE_CONFIG_CONTENT";

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
        CLAUDE_ACP | OPENCODE => Confinement::WithdrawsNativeTools,
        _ => Confinement::CannotWithdraw,
    }
}

/// The confinement an ACP launch of `spec` supports, from its ACP deny slot
/// ([`CodingAgentSpec::acp_tool_deny`], `#agent-tools 11/12`): `acp_meta` and `config_env` withdraw
/// the declared `acp.withdraw_native`; `unsupported` — and a flag, which an ACP agent cannot take,
/// or a spec with no `acp` section — is [`Confinement::CannotWithdraw`].
///
/// TODO(acp-agents): green makes this the only answer — the launcher and `tddy-session-agents`
/// read it instead of [`confinement_for`], and `CLAUDE_NATIVE_TOOLS`, `OPENCODE_NATIVE_TOOLS`,
/// [`withdrawal_session_meta`] and [`opencode_withdrawal_config`] are deleted (items, not files).
pub fn confinement_of_spec(spec: &CodingAgentSpec) -> Confinement {
    // TODO(acp-agents): implement.
    let _ = spec;
    todo!("acp-agents: confinement_of_spec")
}

/// [`refuse_unless_confinable`] for an ACP agent declared as `spec`: refused for a managed session
/// when [`confinement_of_spec`] is [`Confinement::CannotWithdraw`]. `confinable` names the agents
/// that can be confined (the registry's ACP agents whose spec withdraws), in the order given. `Err`
/// is the reason, naming the spec's declaration:
///
/// `the 'codex-acp' agent cannot withdraw its native file-system and shell tools (its spec declares
/// tool_deny: unsupported), and a managed session's main agent has none, so its subagent must not
/// either: attach a coding agent that can be confined (claude-acp, opencode) or a model agent`
pub fn refuse_unless_spec_confinable(
    spec: &CodingAgentSpec,
    managed: bool,
    confinable: &[&str],
) -> Result<(), String> {
    // TODO(acp-agents): implement.
    let _ = (spec, managed, confinable);
    todo!("acp-agents: refuse_unless_spec_confinable")
}

/// Refuse `harness` for a managed session when its native tools cannot be withdrawn: the session's
/// main agent has none, so a subagent must not either. `Err` is the reason, naming the harnesses
/// that can be confined; the caller says which agent and which session it refused.
pub fn refuse_unless_confinable(harness: &str, managed: bool) -> Result<(), String> {
    if !managed || confinement_for(harness) == Confinement::WithdrawsNativeTools {
        return Ok(());
    }
    let confinable: Vec<&str> = ACP_HARNESSES
        .iter()
        .copied()
        .filter(|candidate| confinement_for(candidate) == Confinement::WithdrawsNativeTools)
        .collect();
    Err(format!(
        "the '{harness}' harness's native file-system and shell tools cannot be withdrawn, and a \
         managed session's main agent has none, so its subagent must not either: attach a coding \
         agent on a harness that can be confined ({}) or a model agent",
        confinable.join(", ")
    ))
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
/// harness withdraws them another way (see [`opencode_withdrawal_config`]) or cannot.
///
/// `claude-acp`: `claude-agent-acp` merges `_meta.claudeCode.options` into the Claude Code SDK
/// options, whose `disallowedTools` removes a tool outright. DOCUMENTED, not verified against a live
/// adapter; that `_meta` reaches `session/new` is verified against the stub.
pub fn withdrawal_session_meta(
    harness: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    if harness != CLAUDE_ACP {
        return None;
    }
    let mut meta = serde_json::Map::new();
    meta.insert(
        "claudeCode".to_string(),
        serde_json::json!({ "options": { "disallowedTools": CLAUDE_NATIVE_TOOLS } }),
    );
    Some(meta)
}

/// The inline OpenCode config ([`OPENCODE_CONFIG_CONTENT`]) that withdraws `harness`'s native
/// tools; `None` when the harness withdraws them through [`withdrawal_session_meta`] or cannot.
///
/// `opencode`: `OPENCODE_CONFIG_CONTENT` is inline JSON config merged over the others, and its
/// `tools` table switches built-in tools off. The launcher merges this into whatever inline config
/// the provider route already set. DOCUMENTED, not verified against a live OpenCode.
pub fn opencode_withdrawal_config(harness: &str) -> Option<serde_json::Value> {
    if harness != OPENCODE {
        return None;
    }
    let tools: serde_json::Map<String, serde_json::Value> = OPENCODE_NATIVE_TOOLS
        .iter()
        .map(|tool| ((*tool).to_string(), serde_json::Value::Bool(false)))
        .collect();
    Some(serde_json::json!({ "tools": tools }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CODEX_ACP;

    #[test]
    fn claude_withdraws_its_native_tools_through_session_meta() {
        // When
        let meta = withdrawal_session_meta(CLAUDE_ACP).expect("meta");

        // Then
        let tools = &meta["claudeCode"]["options"]["disallowedTools"];
        assert!(tools.as_array().expect("array").iter().any(|t| t == "Bash"));
    }

    #[test]
    fn claude_needs_no_inline_opencode_config() {
        // When
        let config = opencode_withdrawal_config(CLAUDE_ACP);

        // Then
        assert_eq!(config, None);
    }

    #[test]
    fn opencode_withdraws_its_native_tools_through_inline_config() {
        // When
        let config = opencode_withdrawal_config(OPENCODE).expect("config");

        // Then
        assert_eq!(config["tools"]["bash"], false);
    }

    #[test]
    fn opencode_needs_no_session_meta() {
        // When
        let meta = withdrawal_session_meta(OPENCODE);

        // Then
        assert!(meta.is_none());
    }

    #[test]
    fn a_harness_that_cannot_withdraw_gets_no_session_meta() {
        // When
        let meta = withdrawal_session_meta(CODEX_ACP);

        // Then
        assert!(meta.is_none());
    }

    #[test]
    fn a_harness_that_cannot_withdraw_gets_no_inline_config() {
        // When
        let config = opencode_withdrawal_config(CODEX_ACP);

        // Then
        assert_eq!(config, None);
    }

    #[test]
    fn an_unconfinable_harness_is_admitted_to_an_unmanaged_session() {
        // When
        let admitted = refuse_unless_confinable(CODEX_ACP, false);

        // Then
        assert_eq!(admitted, Ok(()));
    }

    #[test]
    fn a_confinable_harness_is_admitted_to_a_managed_session() {
        // When
        let admitted = refuse_unless_confinable(OPENCODE, true);

        // Then
        assert_eq!(admitted, Ok(()));
    }

    #[test]
    fn an_unconfinable_harness_is_refused_in_a_managed_session_naming_the_confinable_ones() {
        // When
        let refusal = refuse_unless_confinable(CODEX_ACP, true).expect_err("refused");

        // Then
        assert!(
            refusal.contains("'codex-acp'") && refusal.contains("(claude-acp, opencode)"),
            "{refusal}"
        );
    }
}
