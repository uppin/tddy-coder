//! The `claude-acp`, `codex-acp` and `opencode` workflow backends from their tool definitions:
//! `claude-acp.yaml`'s, `codex-acp.yaml`'s and `opencode.yaml`'s `coding_agent.acp` sections render
//! the agent command, and the hooks adapters they name run around the driver
//! (`#agent-tools 11/12`).
//!
//! PRD: `docs/ft/coder/changelog/2026-11-04-acp-agents-from-specs.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context};
use tddy_actions::tool::ToolHost;
use tddy_coding_agent_specs::{load_agent_registry, package_runner, AgentEnvBase, CodingAgentSpec};
use tddy_core::adapters::{AcpHooksAdapter, AcpHooksAdapters};
use tddy_core::AcpAgentCommand;

/// `codex-acp.yaml`'s `binary.env`: the variable that names the `codex-acp` binary.
pub const CODEX_ACP_BINARY_ENV: &str = "TDDY_CODEX_ACP_CLI";

/// `opencode.yaml`'s `binary.env`: the variable that names the OpenCode binary.
pub const OPENCODE_BINARY_ENV: &str = "TDDY_OPENCODE_CLI";

/// What a spec-declared ACP backend is built from.
pub(crate) struct AcpLaunch {
    /// The command, rendered from the spec's `coding_agent.acp.command`.
    pub(crate) command: AcpAgentCommand,
    /// The hooks adapter the spec names, when it names one.
    pub(crate) hooks: Option<Arc<dyn AcpHooksAdapter>>,
    /// The agent's resolved deps, by tool id (`codex`, the binary a hooks adapter runs).
    pub(crate) deps: BTreeMap<String, PathBuf>,
}

/// The launch of agent `id` on `host`: the registry loaded with `tools_dir` applied, `id` and its
/// deps resolved against `host`, its `coding_agent.acp.command` rendered, and its hooks adapter
/// looked up by the name the spec gives. A provider route is not applied here: a backend applies it
/// when it is set.
pub(crate) fn acp_launch(
    tools_dir: &Path,
    host: &dyn ToolHost,
    id: &str,
) -> anyhow::Result<AcpLaunch> {
    let registry = load_agent_registry(tools_dir).context("the coding agent registry")?;
    let def = registry
        .get(id)
        .ok_or_else(|| anyhow!("no `{id}` tool is declared"))?;
    let spec = CodingAgentSpec::of(def)?;
    // The closure lists deps before their dependents, so the agent's own tool comes last.
    let mut closure = registry.resolve_closure(&[id], host)?;
    let tool = closure
        .pop()
        .ok_or_else(|| anyhow!("`{id}` resolved to no tool"))?;
    let mut command = spec.acp_command(&tool, package_runner(def, host), Vec::new())?;
    command.env =
        AgentEnvBase::load(tools_dir)?.agent_env(&spec, std::env::vars_os(), &command.env);
    let hooks = match spec.acp().and_then(|acp| acp.hooks_adapter.as_deref()) {
        Some(name) => Some(AcpHooksAdapters::builtin().get(name).ok_or_else(|| {
            anyhow!("`{id}` names the hooks adapter `{name}`, which this build does not have")
        })?),
        None => None,
    };
    let deps = closure
        .into_iter()
        .map(|dep| (dep.id, dep.binary))
        .collect();
    Ok(AcpLaunch {
        command,
        hooks,
        deps,
    })
}
