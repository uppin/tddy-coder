//! The `claude-acp`, `codex-acp` and `opencode` workflow backends from their tool definitions:
//! `claude-acp.yaml`'s, `codex-acp.yaml`'s and `opencode.yaml`'s `coding_agent.acp` sections render
//! the agent command, and the hooks adapters they name run around the driver
//! (`#agent-tools 11/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-acp-agents-from-specs.md`.

use std::path::Path;
use std::sync::Arc;

use tddy_actions::tool::ToolHost;
use tddy_core::{ClaudeAcpBackend, CodexAcpBackend, OpenCodeAcpBackend};

/// A `claude-acp` backend from its declaration: the agent registry loaded with `tools_dir`
/// (`<tddyhome>/tools`) applied, `claude-acp` and its deps resolved against `host`, and its
/// `coding_agent.acp.command` rendered — `bunx claude-agent-acp`, or the binary
/// `TDDY_CLAUDE_AGENT_ACP_CLI` names, alone. Refused when the registry does not load, `claude-acp`
/// does not resolve, or its spec has no `acp` section.
pub fn claude_acp_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<ClaudeAcpBackend> {
    // TODO(acp-agents): implement — `create_backend("claude-acp")` then builds it.
    let _ = (tools_dir, host);
    todo!("acp-agents: claude_acp_backend_from_spec")
}

/// A `codex-acp` backend from its declaration, as [`claude_acp_backend_from_spec`] for
/// `codex-acp`: its binary resolved beside its `codex` dep (or named by `TDDY_CODEX_ACP_CLI`), and
/// the `codex-acp-login` hooks running `codex login` with that dep's binary. A provider route is
/// applied afterwards with `with_provider_route`, as today.
pub fn codex_acp_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<CodexAcpBackend> {
    // TODO(acp-agents): implement — and green deletes `codex_binaries::resolve_codex_acp_binary`
    // and `codex_acp_beside_resolved_codex`, which the spec's `binary.sibling_of` replaces.
    let _ = (tools_dir, host);
    todo!("acp-agents: codex_acp_backend_from_spec")
}

/// An `opencode` backend from `opencode.yaml`'s `coding_agent.acp`, as
/// [`claude_acp_backend_from_spec`]: `opencode acp`, its route's file written and held by the
/// `opencode-route-file` hooks. `TDDY_OPENCODE_CLI` names the binary through the spec's
/// `binary.env`.
pub fn opencode_acp_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<OpenCodeAcpBackend> {
    // TODO(acp-agents): implement.
    let _ = (tools_dir, host);
    todo!("acp-agents: opencode_acp_backend_from_spec")
}
