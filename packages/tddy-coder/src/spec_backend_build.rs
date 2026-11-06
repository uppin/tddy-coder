//! `tddy-coder --agent <id>`: the workflow backend of any coding-agent registry id
//! (`#agent-tools 16/19`). Replaces `run::create_backend`'s match on agent ids and
//! `run::BUILTIN_BACKEND_AGENT_IDS`: an agent is selected by its registry id and built from its
//! spec, so `oz` and an agent dropped into `<tddyhome>/tools/` run a workflow with no Rust change.
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-spec-backend.md`.
//!
//! **No fallback**: an id no registry agent answers to is refused listing
//! [`crate::agent_registry_cli::backend_agent_ids`]; an agent whose spec declares neither a
//! `headless` nor an `acp` section is refused naming it. Neither ever runs as `claude`.

use anyhow::Context as _;
use std::path::Path;
use std::sync::Arc;

use tddy_actions::tool::ToolHost;
use tddy_coding_agent_specs::{routed_agent, CodingAgentRegistry};
use tddy_core::provider_route::HarnessRoute;
use tddy_core::{SharedBackend, SpecAcpBackend, SpecBackend, SpecTransport};

use crate::agent_registry_cli::{backend_agent_ids, STUB_BACKEND_ID};

/// What a caller adds to `--agent <id>` when it builds the backend.
#[derive(Debug, Clone, Default)]
pub struct SpecBackendRequest {
    /// The transport the session or request asks for; `None` takes the spec's rule — headless
    /// when the spec declares both (`SpecTransport::chosen`).
    pub transport: Option<SpecTransport>,
    /// The provider route the session runs on (`--provider-route-path`). Refused for an agent whose
    /// spec has no `coding_agent.route`, and for `stub`.
    pub provider_route: Option<HarnessRoute>,
}

/// The [`SpecBackend`] of registry agent `id`: the registry loaded from `tools_dir`
/// (`<tddyhome>/tools`, builtins included), its transport chosen by `SpecTransport::chosen`, and
/// that transport's backend built — `headless_from_spec::headless_backend_from_spec` for headless,
/// the spec's rendered `acp.command`, hooks adapter and deps in a `SpecAcpBackend` for ACP — with
/// `id` and its deps resolved against `host`.
///
/// Refused when the registry does not load, for `stub` (no spec stands for it), for an id no agent
/// answers to (listing the buildable ids), for a spec with neither section and for a transport
/// asked for that the spec does not declare.
pub fn spec_backend_for(
    tools_dir: &Path,
    id: &str,
    host: Arc<dyn ToolHost>,
    transport: Option<SpecTransport>,
) -> anyhow::Result<SpecBackend> {
    let registry = CodingAgentRegistry::load(tools_dir).context("the coding agent registry")?;
    // A PTY-only agent is found here and refused by `chosen`, naming it.
    let Some(spec) = registry.get(id) else {
        anyhow::bail!(
            "unknown agent {id:?}: no coding agent of that id can run a workflow turn on this \
             host. Known agents: {}",
            backend_agent_ids(&registry).join(", ")
        );
    };
    let chosen = SpecTransport::chosen(
        id,
        spec.headless().is_some(),
        spec.acp().is_some(),
        transport,
    )?;
    match chosen {
        SpecTransport::Headless => {
            crate::headless_from_spec::headless_backend_from_spec(id, tools_dir, host)
                .map(SpecBackend::Headless)
        }
        SpecTransport::Acp => {
            let launch = crate::acp_from_spec::acp_launch(tools_dir, host.as_ref(), id)?;
            let backend = SpecAcpBackend::new(routed_agent(spec), launch.command);
            Ok(SpecBackend::Acp(match launch.hooks {
                Some(hooks) => backend.with_hooks(hooks, launch.deps),
                None => backend,
            }))
        }
    }
}

/// The workflow backend `--agent <id>` runs: `stub` → the simulated-response `StubBackend`
/// (refusing a provider route: it makes no provider call), any other id → [`spec_backend_for`]
/// with `request.provider_route` applied.
///
/// An unknown id is refused as `unknown agent "<id>": no coding agent of that id can run a
/// workflow turn on this host. Known agents: <backend_agent_ids, comma-separated>`.
pub fn build_spec_backend(
    tools_dir: &Path,
    id: &str,
    host: Arc<dyn ToolHost>,
    request: SpecBackendRequest,
) -> anyhow::Result<SharedBackend> {
    if id == STUB_BACKEND_ID {
        anyhow::ensure!(
            request.provider_route.is_none(),
            "the stub backend cannot run on a provider account: it makes no provider call"
        );
        return Ok(SharedBackend::stub());
    }
    let backend = spec_backend_for(tools_dir, id, host, request.transport)?;
    let backend = match request.provider_route {
        Some(route) => backend.with_provider_route(route)?,
        None => backend,
    };
    Ok(SharedBackend::from_arc(Arc::new(backend)))
}

/// The `--*-path` flags that name a binary a spec resolves through its `binary.env`.
#[derive(Debug, Clone, Copy, Default)]
pub struct BinaryPathFlags<'a> {
    /// `--cursor-agent-path`.
    pub cursor_agent: Option<&'a Path>,
    /// `--codex-cli-path`.
    pub codex_cli: Option<&'a Path>,
    /// `--codex-acp-cli-path`.
    pub codex_acp_cli: Option<&'a Path>,
    /// `--opencode-cli-path`.
    pub opencode_cli: Option<&'a Path>,
}

/// The workflow backend `--agent <agent>` runs, as `tddy-coder` starts it: a resolved specialized
/// agent def first (its own endpoint — a provider route is refused for it), otherwise
/// [`build_spec_backend`] on a host the `--*-path` flags name binaries on. Never falls through to
/// another agent.
pub fn create_workflow_backend(
    agent: &str,
    flags: BinaryPathFlags<'_>,
    tools_dir: &Path,
    tool_host: Arc<dyn ToolHost>,
    specialized_agent_defs: &[tddy_discovery::agent_def::SpecializedAgentDef],
    provider_route: Option<HarnessRoute>,
) -> anyhow::Result<SharedBackend> {
    log::info!("[tddy-coder] using agent: {}", agent);
    if let Some(def) = specialized_agent_defs.iter().find(|d| d.name == agent) {
        crate::provider_route_arg::refuse_route_for_definition(agent, provider_route.as_ref())?;
        // The def is the whole configuration — endpoint, model, turn budget and credential. No
        // flag can override it (docs/ft/daemon/session-agent-roster.md AC46).
        log::info!(
            "[tddy-coder] specialized agent backend '{}': url={} model={} max_turns={}",
            def.name,
            def.base_url,
            def.model,
            def.max_turns
        );
        let backend = tddy_discovery::backend::SpecializedAgentBackend::from_def(def);
        return Ok(SharedBackend::from_arc(Arc::new(backend)));
    }
    use crate::acp_from_spec::{CODEX_ACP_BINARY_ENV, OPENCODE_BINARY_ENV};
    use crate::headless_from_spec::{host_naming_binary, CODEX_BINARY_ENV, CURSOR_BINARY_ENV};
    let host = host_naming_binary(tool_host, CURSOR_BINARY_ENV, flags.cursor_agent);
    let host = host_naming_binary(host, CODEX_BINARY_ENV, flags.codex_cli);
    let host = host_naming_binary(host, CODEX_ACP_BINARY_ENV, flags.codex_acp_cli);
    let host = host_naming_binary(host, OPENCODE_BINARY_ENV, flags.opencode_cli);
    build_spec_backend(
        tools_dir,
        agent,
        host,
        SpecBackendRequest {
            transport: None,
            provider_route,
        },
    )
}
