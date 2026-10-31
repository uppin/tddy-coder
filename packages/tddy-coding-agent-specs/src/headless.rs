//! Headless coding agents from their declaration: `coding_agent.headless` rendered into the
//! [`HeadlessInvocation`] `tddy-agent-backend`'s consumers spawn (`#agent-tools 6/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-agents-from-specs.md`.
//!
//! The backend owns the port ([`HeadlessRenderer`]) and the adapter registry; this crate depends on
//! the backend, so it implements the port here and validates adapter names against
//! [`AdapterRegistry`] directly — no dependency points back.

use tddy_actions::tool::ResolvedTool;
use tddy_agent_backend::adapters::{
    AdapterError, AdapterRegistry, HeadlessInvocation, HeadlessMcpServer, HeadlessRenderer,
    HeadlessTurn,
};
use thiserror::Error;

use crate::{CodingAgentSpec, HeadlessSection};

/// The id of the dep a headless agent's tool server runs (`tddy-tools --mcp`).
pub const TOOL_SERVER_DEP: &str = "tddy-tools";

/// Why an agent cannot be launched headless.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HeadlessSpecError {
    /// The agent declares no `coding_agent.headless` section.
    #[error("coding agent {agent} has no headless section")]
    NoHeadlessSection { agent: String },
    /// Its `headless.adapter` names no adapter this build registers.
    #[error("coding agent {agent}: headless adapter {adapter} is not registered (registered: {registered})")]
    UnknownAdapter {
        agent: String,
        adapter: String,
        /// The registered names, comma-joined.
        registered: String,
    },
}

/// `section`'s adapter checked against `adapters`: refused, naming the adapter and what is
/// registered, when no adapter of that name is.
pub fn validate_headless(
    agent: &str,
    section: &HeadlessSection,
    adapters: &AdapterRegistry,
) -> Result<(), HeadlessSpecError> {
    // TODO(claude-headless): implement.
    let _ = (agent, section, adapters);
    todo!("claude-headless: validate_headless")
}

/// A [`HeadlessRenderer`] over one agent's declaration, as this host resolved it.
#[derive(Debug, Clone)]
pub struct SpecHeadlessRenderer {
    spec: CodingAgentSpec,
    /// The agent's own tool (`{binary}`).
    tool: ResolvedTool,
    /// Its resolved [`TOOL_SERVER_DEP`], when it has one.
    tool_server: Option<ResolvedTool>,
    adapters: AdapterRegistry,
}

impl SpecHeadlessRenderer {
    /// A renderer for `spec`, resolved as `tool` with its deps `deps`. Refused when the spec has no
    /// headless section or names an adapter `adapters` does not hold.
    pub fn new(
        spec: CodingAgentSpec,
        tool: ResolvedTool,
        deps: &[ResolvedTool],
        adapters: AdapterRegistry,
    ) -> Result<Self, HeadlessSpecError> {
        // TODO(claude-headless): implement — `validate_headless`, then keep the `tddy-tools` dep.
        let _ = (spec, tool, deps, adapters);
        todo!("claude-headless: SpecHeadlessRenderer::new")
    }
}

impl HeadlessRenderer for SpecHeadlessRenderer {
    fn agent_id(&self) -> &str {
        &self.spec.id
    }

    fn tool_server(&self) -> Option<HeadlessMcpServer> {
        // TODO(claude-headless): implement — `tddy-tools --mcp` from the resolved dep, no env.
        let _ = &self.tool_server;
        todo!("claude-headless: SpecHeadlessRenderer::tool_server")
    }

    /// `slots::render` over the turn's slot inputs (no preamble, no hooks), then
    /// `render_argv(headless.run_section(), tool, params)` with `prompt`, `model`, `session_id`,
    /// `resume`, `permission_mode` and the rendered slot flags as `slots`; `{binary}` (the first
    /// rendered entry) becomes the program.
    fn render(&self, turn: &HeadlessTurn) -> Result<HeadlessInvocation, AdapterError> {
        // TODO(claude-headless): implement.
        let _ = (turn, &self.tool, &self.adapters);
        todo!("claude-headless: SpecHeadlessRenderer::render")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;
    use tddy_actions::tool::ArgvItem;

    use super::*;

    fn a_headless_section(adapter: &str) -> HeadlessSection {
        HeadlessSection {
            argv: vec![ArgvItem::Lit("{binary}".to_string())],
            env: BTreeMap::new(),
            adapter: adapter.to_string(),
            exit_ok_if_structured: false,
        }
    }

    #[test]
    fn a_headless_section_naming_a_registered_adapter_validates() {
        // Given
        let section = a_headless_section("claude-stream-json");

        // When
        let validated = validate_headless("claude", &section, &AdapterRegistry::builtin());

        // Then
        assert_eq!(validated, Ok(()));
    }

    #[test]
    fn a_headless_section_naming_an_unregistered_adapter_is_refused_naming_it_and_the_registered_ones(
    ) {
        // Given
        let section = a_headless_section("claude-ndjson-v2");

        // When
        let validated = validate_headless("claude", &section, &AdapterRegistry::builtin());

        // Then
        assert_eq!(
            validated,
            Err(HeadlessSpecError::UnknownAdapter {
                agent: "claude".to_string(),
                adapter: "claude-ndjson-v2".to_string(),
                registered: "claude-stream-json".to_string(),
            })
        );
    }
}
