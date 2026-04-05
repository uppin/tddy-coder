//! Granular RED tests for session semantic search extraction + schema contracts (PRD Testing Plan).

use tddy_core::changeset::Changeset;
use tddy_core::session_semantic_search::{
    build_index_document_text, merge_branch_label, merge_worktree_label,
    SESSION_SEARCH_EMBEDDING_MODEL_ID, SESSION_SEARCH_INDEX_SCHEMA_VERSION,
};

fn minimal_changeset() -> Changeset {
    Changeset::default()
}

/// RED: migrations must ship with schema version ≥ 1 once the SQLite DDL exists.
#[test]
fn session_search_index_schema_version_is_published() {
    assert!(
        SESSION_SEARCH_INDEX_SCHEMA_VERSION >= 1,
        "SESSION_SEARCH_INDEX_SCHEMA_VERSION must be bumped when the initial schema lands"
    );
}

/// RED: embedding model id must be pinned for rebuild/migration workflows.
#[test]
fn session_search_embedding_model_id_is_non_empty() {
    assert!(
        !SESSION_SEARCH_EMBEDDING_MODEL_ID.trim().is_empty(),
        "SESSION_SEARCH_EMBEDDING_MODEL_ID must name a concrete model"
    );
}

/// RED: explicit worktree wins over suggestion.
#[test]
fn merge_worktree_prefers_explicit_over_suggestion() {
    let mut cs = minimal_changeset();
    cs.worktree = Some("/wt/explicit".to_string());
    cs.worktree_suggestion = Some("suggested-wt".to_string());
    assert_eq!(merge_worktree_label(&cs), "/wt/explicit");
}

/// RED: fall back to suggestion when explicit worktree unset.
#[test]
fn merge_worktree_falls_back_to_suggestion() {
    let mut cs = minimal_changeset();
    cs.worktree_suggestion = Some("suggested-only".to_string());
    assert_eq!(merge_worktree_label(&cs), "suggested-only");
}

/// RED: explicit branch wins over suggestion.
#[test]
fn merge_branch_prefers_explicit_over_suggestion() {
    let mut cs = minimal_changeset();
    cs.branch = Some("feature/explicit".to_string());
    cs.branch_suggestion = Some("feature/suggestion".to_string());
    assert_eq!(merge_branch_label(&cs), "feature/explicit");
}

/// RED: index document includes prompt-derived content when initial_prompt is set.
#[test]
fn build_index_document_text_includes_initial_prompt() {
    let mut cs = minimal_changeset();
    cs.initial_prompt = Some("hello world fixture".to_string());
    let doc = build_index_document_text(&cs);
    assert!(
        doc.contains("hello world fixture"),
        "index document must include initial_prompt for embedding"
    );
}
