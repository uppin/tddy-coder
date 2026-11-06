//! The `claude-acp`, `codex-acp` and `opencode` workflow backends from their tool definitions:
//! `claude-acp.yaml`'s, `codex-acp.yaml`'s and `opencode.yaml`'s `coding_agent.acp` sections render
//! the agent command, and the hooks adapters they name run around the driver
//! (`#agent-tools 11/12`).
//!
//! PRD: `docs/ft/coder/changelog/2026-11-04-acp-agents-from-specs.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, ensure, Context};
use tddy_actions::tool::ToolHost;
use tddy_coding_agent_specs::{load_agent_registry, package_runner, CodingAgentSpec};
use tddy_core::adapters::{AcpHooksAdapter, AcpHooksAdapters};
use tddy_core::{AcpAgentCommand, ClaudeAcpBackend, CodexAcpBackend, OpenCodeAcpBackend};

/// `codex-acp.yaml`'s `binary.env`: the variable that names the `codex-acp` binary.
pub const CODEX_ACP_BINARY_ENV: &str = "TDDY_CODEX_ACP_CLI";

/// `opencode.yaml`'s `binary.env`: the variable that names the OpenCode binary.
pub const OPENCODE_BINARY_ENV: &str = "TDDY_OPENCODE_CLI";

/// What a spec-declared ACP backend is built from.
struct AcpLaunch {
    /// The command, rendered from the spec's `coding_agent.acp.command`.
    command: AcpAgentCommand,
    /// The hooks adapter the spec names, when it names one.
    hooks: Option<Arc<dyn AcpHooksAdapter>>,
    /// The agent's resolved deps, by tool id (`codex`, the binary a hooks adapter runs).
    deps: BTreeMap<String, PathBuf>,
}

/// The launch of agent `id` on `host`: the registry loaded with `tools_dir` applied, `id` and its
/// deps resolved against `host`, its `coding_agent.acp.command` rendered, and its hooks adapter
/// looked up by the name the spec gives. A provider route is not applied here: a backend applies it
/// when it is set.
fn acp_launch(tools_dir: &Path, host: &dyn ToolHost, id: &str) -> anyhow::Result<AcpLaunch> {
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
    let command = spec.acp_command(&tool, package_runner(def, host), Vec::new())?;
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

/// A `claude-acp` backend from its declaration: the agent registry loaded with `tools_dir`
/// (`<tddyhome>/tools`) applied, `claude-acp` and its deps resolved against `host`, and its
/// `coding_agent.acp.command` rendered — `bunx claude-agent-acp`, or the binary
/// `TDDY_CLAUDE_AGENT_ACP_CLI` names, alone. Refused when the registry does not load, `claude-acp`
/// does not resolve, or its spec has no `acp` section.
pub fn claude_acp_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<ClaudeAcpBackend> {
    let launch = acp_launch(tools_dir, host.as_ref(), "claude-acp")?;
    ensure!(
        launch.hooks.is_none(),
        "claude-acp's spec names a hooks adapter, which its backend does not run"
    );
    Ok(ClaudeAcpBackend::with_command(launch.command))
}

/// A `codex-acp` backend from its declaration, as [`claude_acp_backend_from_spec`] for
/// `codex-acp`: its binary resolved beside its `codex` dep (or named by `TDDY_CODEX_ACP_CLI`), and
/// the `codex-acp-login` hooks running `codex login` with that dep's binary. A provider route is
/// applied afterwards with `with_provider_route`, as today.
pub fn codex_acp_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<CodexAcpBackend> {
    let launch = acp_launch(tools_dir, host.as_ref(), "codex-acp")?;
    let hooks = launch.hooks.ok_or_else(|| {
        anyhow!("codex-acp's spec names no hooks adapter, so it has no login retry")
    })?;
    Ok(CodexAcpBackend::with_launch(
        launch.command,
        hooks,
        launch.deps,
    ))
}

/// An `opencode` backend from `opencode.yaml`'s `coding_agent.acp`, as
/// [`claude_acp_backend_from_spec`]: `opencode acp`, its route's file written and held by the
/// `opencode-route-file` hooks. `TDDY_OPENCODE_CLI` names the binary through the spec's
/// `binary.env`.
pub fn opencode_acp_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<OpenCodeAcpBackend> {
    let launch = acp_launch(tools_dir, host.as_ref(), "opencode")?;
    let hooks = launch.hooks.ok_or_else(|| {
        anyhow!("opencode's spec names no hooks adapter, so its route file has no holder")
    })?;
    Ok(OpenCodeAcpBackend::with_launch(
        launch.command,
        hooks,
        launch.deps,
    ))
}
