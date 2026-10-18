//! `opencode` is registered wherever a tool-session backend is listed, after every existing entry.
//!
//! Feature: docs/ft/coder/1-WIP/PRD-2026-10-18-provider-route-opencode-acp.md

use tddy_agent_backend::backend::{
    backend_from_label, backend_selection_question, curated_models_for_agent,
    default_model_for_agent,
};

#[test]
fn backend_selection_question_appends_opencode() {
    // When
    let labels: Vec<String> = backend_selection_question()
        .options
        .iter()
        .map(|option| option.label.clone())
        .collect();

    // Then: seven options, the six existing ones unchanged and first
    assert_eq!(labels.len(), 7, "{labels:?}");
    assert!(labels[6].contains("OpenCode"), "{labels:?}");
    assert_eq!(backend_from_label(&labels[6]).0, "opencode");
}

#[test]
fn opencode_has_a_default_model_in_its_curated_list() {
    // When
    let default = default_model_for_agent("opencode");
    let curated = curated_models_for_agent("opencode");

    // Then: an OpenCode model id (`provider/model`), not the Claude fallback
    assert_ne!(default, default_model_for_agent("claude"));
    assert!(default.contains('/'), "{default}");
    assert!(
        curated.models.iter().any(|model| model.id == default),
        "{default}"
    );
}
