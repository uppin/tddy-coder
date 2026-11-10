//! The ACP slots (`#agent-tools 11/12`): the tool server as `session/new` `mcp_servers`, and native
//! tools withdrawn through `_meta` or inline config, as a spec's [`crate::AcpSection`] declares.
//!
//! PRD: `docs/ft/coder/changelog/2026-11-04-acp-agents-from-specs.md`.

use tddy_agent_backend::McpServerSpec;

use super::{McpServerCommand, SlotError};
use crate::spec::slot_shapes::McpSlot;
use crate::spec::slot_shapes::ToolSlot;
use crate::CodingAgentSpec;

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
    let Some(server) = server else {
        return Ok(Vec::new());
    };
    match spec.acp_mcp() {
        Some(McpSlot::AcpSession) => Ok(vec![McpServerSpec {
            name: server.name.clone(),
            command: server.command.clone(),
            args: server.args.clone(),
            // `BTreeMap` iterates in name order, which is the order the entries are sent in.
            env: server
                .env
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            cwd: None,
        }]),
        _ => Err(SlotError::SlotUnsupported {
            agent: spec.id.clone(),
            slot: "mcp".to_string(),
        }),
    }
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
    if tools.is_empty() {
        return Ok(None);
    }
    let refused = || SlotError::SlotUnsupported {
        agent: spec.id.clone(),
        slot: "tool_deny".to_string(),
    };
    match spec.acp_tool_deny().ok_or_else(refused)? {
        ToolSlot::AcpMeta { path } => Ok(Some(AcpWithdrawal::Meta(meta_at(path, tools)))),
        ToolSlot::ConfigEnv { env } => Ok(Some(AcpWithdrawal::ConfigEnv {
            env: env.clone(),
            config: switched_off_config(tools),
        })),
        ToolSlot::Flag(_) | ToolSlot::Unsupported(_) => Err(refused()),
    }
}

/// `tools` as a list at the dotted `path` of a JSON object, nested as the path is:
/// `claudeCode.options.disallowedTools` → `{"claudeCode": {"options": {"disallowedTools": [...]}}}`.
fn meta_at(path: &str, tools: &[String]) -> serde_json::Map<String, serde_json::Value> {
    let list = serde_json::Value::Array(
        tools
            .iter()
            .cloned()
            .map(serde_json::Value::String)
            .collect(),
    );
    let mut keys = path.split('.').rev();
    // `split` yields the whole path at least once, so the leaf is always there.
    let leaf = keys.next().unwrap_or(path);
    keys.fold(
        serde_json::Map::from_iter([(leaf.to_string(), list)]),
        |inner, key| {
            serde_json::Map::from_iter([(key.to_string(), serde_json::Value::Object(inner))])
        },
    )
}

/// `{"tools": {<tool>: false, ...}}`: each withdrawn tool switched off in OpenCode's inline config.
fn switched_off_config(tools: &[String]) -> serde_json::Value {
    let switched_off: serde_json::Map<String, serde_json::Value> = tools
        .iter()
        .map(|tool| (tool.clone(), serde_json::Value::Bool(false)))
        .collect();
    serde_json::json!({ "tools": switched_off })
}
