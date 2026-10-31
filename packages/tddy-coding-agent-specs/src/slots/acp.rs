//! The ACP slots (`#agent-tools 11/12`): the tool server as `session/new` `mcp_servers`, and native
//! tools withdrawn through `_meta` or inline config, as a spec's [`crate::AcpSection`] declares.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-acp-agents-from-specs.md`.

use tddy_agent_backend::McpServerSpec;

use super::{McpServerCommand, SlotError};
use crate::{CodingAgentSpec, McpSlot};

/// How an ACP launch withdraws an agent's native tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpWithdrawal {
    /// Sent as `session/new` `_meta`: the tools as a list at the slot's dotted path —
    /// `{"claudeCode": {"options": {"disallowedTools": [...]}}}` for `claudeCode.options.disallowedTools`.
    Meta(serde_json::Map<String, serde_json::Value>),
    /// Inline JSON config `{"tools": {<tool>: false, ...}}` handed in variable `env`, merged over
    /// whatever inline config the provider route set (OpenCode's `OPENCODE_CONFIG_CONTENT`).
    ConfigEnv {
        env: String,
        config: serde_json::Value,
    },
}

/// The `mcp_servers` an ACP session of `spec` is opened with: `server` as the one entry for an
/// [`McpSlot::AcpSession`] slot, nothing when `server` is `None`. Refused as
/// [`SlotError::SlotUnsupported`] (`slot: "mcp"`) when `server` is given and the agent's ACP MCP
/// slot ([`CodingAgentSpec::acp_mcp`]) is any other kind, or it has no ACP mode.
pub fn acp_mcp_servers(
    spec: &CodingAgentSpec,
    server: Option<&McpServerCommand>,
) -> Result<Vec<McpServerSpec>, SlotError> {
    // TODO(acp-agents): implement — `McpServerCommand` → `McpServerSpec`, its `env` in name order.
    let _ = (spec, server, McpSlot::AcpSession);
    todo!("acp-agents: acp_mcp_servers")
}

/// How an ACP launch of `spec` withdraws `tools` — the declared `acp.withdraw_native` on a managed
/// launch — through its ACP deny slot ([`CodingAgentSpec::acp_tool_deny`]): `acp_meta` as
/// [`AcpWithdrawal::Meta`], `config_env` as [`AcpWithdrawal::ConfigEnv`]. `None` when `tools` is
/// empty. Refused as [`SlotError::SlotUnsupported`] (`slot: "tool_deny"`) for tools and a slot that
/// is `unsupported` or a flag (an ACP agent takes no argv flags per tool), or an agent with no ACP
/// mode — never dropped.
pub fn acp_withdrawal(
    spec: &CodingAgentSpec,
    tools: &[String],
) -> Result<Option<AcpWithdrawal>, SlotError> {
    // TODO(acp-agents): implement.
    let _ = (spec, tools);
    todo!("acp-agents: acp_withdrawal")
}
