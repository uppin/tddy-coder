//! The `claude` workflow backend from its tool definition: `claude.yaml`'s `coding_agent.headless`
//! section renders each invocation, and the adapter it names reads the output
//! (`#agent-tools 6/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-agents-from-specs.md`.

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use tddy_actions::tool::ToolHost;
use tddy_coding_agent_specs::headless::SpecHeadlessRenderer;
use tddy_coding_agent_specs::{load_agent_registry, CodingAgentSpec};
use tddy_core::adapters::AdapterRegistry;
use tddy_core::ClaudeCodeBackend;

/// The coding agent this backend is declared as.
const CLAUDE: &str = "claude";

/// A `claude` backend rendered from its declaration: the agent registry loaded with
/// `tools_dir` (`<tddyhome>/tools`) applied, `claude` and its deps resolved against `host`, and a
/// `SpecHeadlessRenderer` over the result installed with `with_headless_renderer`. Refused when
/// the registry does not load, `claude` does not resolve, or its spec has no usable headless
/// section.
pub fn claude_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<ClaudeCodeBackend> {
    let registry = load_agent_registry(tools_dir).context("the coding agent registry")?;
    let def = registry
        .get(CLAUDE)
        .ok_or_else(|| anyhow!("no `{CLAUDE}` tool is declared"))?;
    let spec = CodingAgentSpec::of(def)?;
    // The closure lists deps before their dependents, so the agent's own tool comes last.
    let mut closure = registry.resolve_closure(&[CLAUDE], host.as_ref())?;
    let tool = closure
        .pop()
        .ok_or_else(|| anyhow!("`{CLAUDE}` resolved to no tool"))?;
    let renderer = SpecHeadlessRenderer::new(spec, tool, &closure, AdapterRegistry::builtin())?;
    Ok(ClaudeCodeBackend::new().with_headless_renderer(Arc::new(renderer)))
}
