//! `tddy-coder --agent` and its interactive selection, read from the coding-agent registry
//! (`#agent-tools 12/12`).
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`. Replaces
//! `run::BUILTIN_BACKEND_AGENT_IDS` and `tddy_agent_backend::backend_selection_question`'s fixed
//! options: a workflow backend can be built from any registry agent that declares a way to take a
//! turn without a terminal — a `coding_agent.headless` or a `coding_agent.acp` section — so an
//! agent dropped into `<tddyhome>/tools/` is selectable with no Rust change.

use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::{ClarificationQuestion, QuestionOption};

/// The test backend with simulated responses, which no spec stands for. Listed after the registry.
pub const STUB_BACKEND_ID: &str = "stub";

/// The ids `--agent` accepts as a coding backend: every registry agent with a `headless` or `acp`
/// section, ordered by id, then [`STUB_BACKEND_ID`]. A PTY-only agent (`warp`, `muse`) is not one.
pub fn backend_agent_ids(registry: &CodingAgentRegistry) -> Vec<String> {
    registry
        .all()
        .into_iter()
        .filter(|spec| spec.headless().is_some() || spec.agent.acp.is_some())
        .map(|spec| spec.id.clone())
        .chain(std::iter::once(STUB_BACKEND_ID.to_string()))
        .collect()
}

/// The interactive backend selection: one option per [`backend_agent_ids`] entry, labelled by its
/// id and described by its spec's `summary`.
pub fn backend_selection_question_from(registry: &CodingAgentRegistry) -> ClarificationQuestion {
    let options = backend_agent_ids(registry)
        .into_iter()
        .map(|id| QuestionOption {
            description: registry
                .tools()
                .get(&id)
                .map(|tool| tool.summary.clone())
                .unwrap_or_else(|| "Test backend with simulated responses".to_string()),
            label: id,
        })
        .collect();
    ClarificationQuestion {
        header: "Backend".to_string(),
        question: "Select the coding backend".to_string(),
        options,
        multi_select: false,
        allow_other: false,
    }
}
