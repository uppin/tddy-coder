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

use std::path::Path;
use std::sync::Arc;

use tddy_actions::tool::ToolHost;
use tddy_core::provider_route::HarnessRoute;
use tddy_core::{SharedBackend, SpecBackend, SpecTransport};

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
    // TODO(spec-backend): implement.
    let _ = (tools_dir, id, host, transport);
    todo!("spec-backend: spec_backend_for")
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
    // TODO(spec-backend): implement.
    let _ = (tools_dir, id, host, request);
    todo!("spec-backend: build_spec_backend")
}
