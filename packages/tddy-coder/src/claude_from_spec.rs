//! The `claude` workflow backend from its tool definition: `claude.yaml`'s `coding_agent.headless`
//! section renders each invocation, and the adapter it names reads the output
//! (`#agent-tools 6/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-agents-from-specs.md`.

use std::path::Path;
use std::sync::Arc;

use tddy_actions::tool::ToolHost;
use tddy_core::ClaudeCodeBackend;

/// A `claude` backend rendered from its declaration: the agent registry loaded with
/// `tools_dir` (`<tddyhome>/tools`) applied, `claude` and its deps resolved against `host`, and a
/// `SpecHeadlessRenderer` over the result installed with `with_headless_renderer`. Refused when
/// the registry does not load, `claude` does not resolve, or its spec has no usable headless
/// section.
pub fn claude_backend_from_spec(
    tools_dir: &Path,
    host: Arc<dyn ToolHost>,
) -> anyhow::Result<ClaudeCodeBackend> {
    // TODO(claude-headless): implement — `load_agent_registry`, `CodingAgentSpec::of`,
    // `resolve_closure(&["claude"], host)`, `SpecHeadlessRenderer::new(…, AdapterRegistry::builtin())`.
    let _ = (tools_dir, host);
    todo!("claude-headless: claude_backend_from_spec")
}
