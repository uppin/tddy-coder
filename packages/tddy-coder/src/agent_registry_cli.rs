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

/// The names an assistant in the model registry may not take, because `--agent <name>` already
/// means a coding agent on this host (`#agent-tools 16/19`): **every** registry agent's id —
/// PTY-only ones (`muse`, `warp`) included, since each is a session type too — ordered by id, then
/// [`STUB_BACKEND_ID`]. The daemon feeds them to `ModelRegistryStore::reserving_coding_agent_ids`,
/// replacing the store's read of `run::BUILTIN_BACKEND_AGENT_IDS`.
pub fn reserved_agent_ids(registry: &CodingAgentRegistry) -> Vec<String> {
    let mut ids: Vec<String> = registry
        .all()
        .into_iter()
        .map(|spec| spec.id.clone())
        .collect();
    ids.sort();
    ids.push(STUB_BACKEND_ID.to_string());
    ids
}

/// The interactive backend selection over the built-in registry: for a caller (the plain
/// stdin menu, the TUI's preselection) that has no host registry of its own loaded.
pub fn builtin_backend_selection_question() -> ClarificationQuestion {
    backend_selection_question_from(CodingAgentRegistry::builtin_cached())
}

/// `(agent id, default model)` for a selection `label` — an agent id from
/// [`backend_selection_question_from`]: its spec's `models.default`, or the id itself for
/// `stub`, which no spec stands for and whose only model is its own name.
pub fn agent_and_default_model(label: &str) -> (String, String) {
    let registry = CodingAgentRegistry::builtin_cached();
    let model = registry
        .get(label)
        .map(|spec| spec.agent.models.default.clone())
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| label.to_string());
    (label.to_string(), model)
}

/// Each selectable agent's default model, for injection into the presenter that answers a
/// backend selection: `coding_agent.models.default` per [`backend_agent_ids`] entry, and the
/// id itself for `stub`.
pub fn backend_default_models(
    registry: &CodingAgentRegistry,
) -> std::collections::BTreeMap<String, String> {
    backend_agent_ids(registry)
        .into_iter()
        .map(|id| {
            let model = registry
                .get(&id)
                .map(|spec| spec.agent.models.default.clone())
                .filter(|model| !model.is_empty())
                .unwrap_or_else(|| id.clone());
            (id, model)
        })
        .collect()
}

/// The default model of `agent` from its built-in spec (`coding_agent.models.default`); empty
/// for an agent no built-in spec stands for, which names no default rather than another agent's.
pub fn default_model_for_agent(agent: &str) -> &'static str {
    CodingAgentRegistry::builtin_cached()
        .get(agent)
        .map(|spec| spec.agent.models.default.as_str())
        .unwrap_or("")
}

/// The index of `agent` in the built-in selection's options, for preselecting it; `0` for an
/// agent the selection does not offer.
pub fn preselected_index_for_agent(agent: &str) -> usize {
    backend_agent_ids(CodingAgentRegistry::builtin_cached())
        .iter()
        .position(|id| id == agent)
        .unwrap_or(0)
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
