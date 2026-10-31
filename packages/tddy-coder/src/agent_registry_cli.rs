//! `tddy-coder --agent` and its interactive selection, read from the coding-agent registry
//! (`#agent-tools 12/12`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`. Replaces
//! `run::BUILTIN_BACKEND_AGENT_IDS` and `tddy_agent_backend::backend_selection_question`'s fixed
//! options: a workflow backend can be built from any registry agent that declares a way to take a
//! turn without a terminal — a `coding_agent.headless` or a `coding_agent.acp` section — so an
//! agent dropped into `<tddyhome>/tools/` is selectable with no Rust change.

use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::ClarificationQuestion;

/// The test backend with simulated responses, which no spec stands for. Listed after the registry.
pub const STUB_BACKEND_ID: &str = "stub";

/// The ids `--agent` accepts as a coding backend: every registry agent with a `headless` or `acp`
/// section, ordered by id, then [`STUB_BACKEND_ID`]. A PTY-only agent (`warp`, `muse`) is not one.
pub fn backend_agent_ids(registry: &CodingAgentRegistry) -> Vec<String> {
    // TODO(agent-registry): implement — and make `run::create_backend` accept exactly these, so
    // `BUILTIN_BACKEND_AGENT_IDS` can be deleted.
    let _ = registry;
    todo!("agent-registry: backend_agent_ids")
}

/// The interactive backend selection: one option per [`backend_agent_ids`] entry, labelled by its
/// id and described by its spec's `summary`.
pub fn backend_selection_question_from(registry: &CodingAgentRegistry) -> ClarificationQuestion {
    // TODO(agent-registry): implement — and have `run.rs` ask this instead of
    // `tddy_agent_backend::backend_selection_question`.
    let _ = registry;
    todo!("agent-registry: backend_selection_question_from")
}
