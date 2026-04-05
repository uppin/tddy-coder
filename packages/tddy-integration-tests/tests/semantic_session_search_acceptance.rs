//! Acceptance tests: semantic session search index + ranking + incremental updates (PRD Testing Plan).
//!
//! These tests exercise SQLite + deterministic local embeddings under [`tddy_core::session_semantic_search`].

mod common;

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tddy_core::changeset::{write_changeset, Changeset, ChangesetState};
use tddy_core::output::{
    create_session_dir_with_id, set_tddy_data_dir_override, TDDY_SESSIONS_DIR_ENV,
};
use tddy_core::session_semantic_search::{
    index_session_for_search, search_sessions_semantic, session_search_index_path,
    SESSION_SEARCH_INDEX_FILENAME,
};
use tddy_core::workflow::ids::WorkflowState;

use common::unique_tddy_data_dir_for_test;

fn write_minimal_changeset(
    session_dir: &std::path::Path,
    session_id: &str,
    initial_prompt: &str,
    worktree: Option<&str>,
    branch: Option<&str>,
    branch_suggestion: Option<&str>,
    worktree_suggestion: Option<&str>,
) {
    let cs = Changeset {
        initial_prompt: Some(initial_prompt.to_string()),
        worktree: worktree.map(String::from),
        branch: branch.map(String::from),
        branch_suggestion: branch_suggestion.map(String::from),
        worktree_suggestion: worktree_suggestion.map(String::from),
        sessions: vec![tddy_core::changeset::SessionEntry {
            id: session_id.to_string(),
            agent: "claude".to_string(),
            tag: "plan".to_string(),
            created_at: "2026-04-05T10:00:00Z".to_string(),
            system_prompt_file: None,
        }],
        state: ChangesetState {
            current: WorkflowState::new("Planned"),
            updated_at: "2026-04-05T10:00:00Z".to_string(),
            history: vec![],
            ..Changeset::default().state
        },
        ..Changeset::default()
    };
    write_changeset(session_dir, &cs).expect("write_changeset");
}

/// `index_persists_under_tddy_data_dir`: first index write creates a versioned SQLite file under the data root.
#[test]
fn index_persists_under_tddy_data_dir() {
    let base = unique_tddy_data_dir_for_test();
    fs::create_dir_all(&base).unwrap();
    let prev = std::env::var(TDDY_SESSIONS_DIR_ENV).ok();
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, base.to_str().unwrap());
    set_tddy_data_dir_override(Some(base.clone()));

    let session_id = "00000000-0007-0000-0000-000000000001";
    let session_dir = create_session_dir_with_id(&base, session_id).expect("session dir");
    write_minimal_changeset(
        &session_dir,
        session_id,
        "fixture prompt",
        Some("/tmp/wt-fixture"),
        Some("feature/fixture"),
        None,
        None,
    );

    index_session_for_search(&base, session_id, &session_dir).expect("index_session_for_search");

    let db_path = session_search_index_path(&base);
    assert!(
        db_path.exists(),
        "expected SQLite index at {} (filename {})",
        db_path.display(),
        SESSION_SEARCH_INDEX_FILENAME
    );
    assert_eq!(
        db_path.file_name().and_then(|n| n.to_str()),
        Some(SESSION_SEARCH_INDEX_FILENAME)
    );
    #[cfg(unix)]
    {
        let meta = fs::metadata(&db_path).expect("stat index");
        let mode = meta.permissions().mode() & 0o777;
        assert!(
            mode & 0o400 != 0,
            "index file must be readable by owner (mode {:o})",
            mode
        );
    }

    if let Some(p) = prev {
        std::env::set_var(TDDY_SESSIONS_DIR_ENV, p);
    } else {
        std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    }
    set_tddy_data_dir_override(None);
}

/// `search_ranks_sessions_by_semantic_similarity`: controlled prompts — semantic order must match (not substring-only).
#[test]
fn search_ranks_sessions_by_semantic_similarity() {
    let base = unique_tddy_data_dir_for_test();
    fs::create_dir_all(&base).unwrap();
    let prev = std::env::var(TDDY_SESSIONS_DIR_ENV).ok();
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, base.to_str().unwrap());
    set_tddy_data_dir_override(Some(base.clone()));

    let id_noise = "00000000-0007-0000-0000-000000000010";
    let id_target = "00000000-0007-0000-0000-000000000020";
    let dir_noise = create_session_dir_with_id(&base, id_noise).unwrap();
    let dir_target = create_session_dir_with_id(&base, id_target).unwrap();

    // Many literal "session" tokens → naive substring ranking favors this row for query "session … oauth".
    write_minimal_changeset(
        &dir_noise,
        id_noise,
        "session session session session session parsing logs",
        Some("/wt/noise"),
        Some("feature/noise"),
        None,
        None,
    );
    // Semantically aligned with OAuth / token security (expected higher rank for the query below).
    write_minimal_changeset(
        &dir_target,
        id_target,
        "OAuth2 access tokens, refresh rotation, and secure user login flows",
        Some("/wt/oauth"),
        Some("feature/oauth-login"),
        None,
        None,
    );

    index_session_for_search(&base, id_noise, &dir_noise).unwrap();
    index_session_for_search(&base, id_target, &dir_target).unwrap();

    let query = "oauth2 token refresh and secure user login";
    let hits = search_sessions_semantic(&base, query).expect("search");
    let ids: Vec<&str> = hits.iter().map(|h| h.session_id.as_str()).collect();

    assert_eq!(
        ids,
        vec![id_target, id_noise],
        "semantic search must rank the OAuth-focused session above the noisy session; \
         substring-only ranking typically prefers the prompt with repeated 'session' tokens"
    );

    if let Some(p) = prev {
        std::env::set_var(TDDY_SESSIONS_DIR_ENV, p);
    } else {
        std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    }
    set_tddy_data_dir_override(None);
}

/// `changeset_update_refreshes_index`: editing `changeset.yaml` must update search without a manual full reindex.
#[test]
fn changeset_update_refreshes_index() {
    let base = unique_tddy_data_dir_for_test();
    fs::create_dir_all(&base).unwrap();
    let prev = std::env::var(TDDY_SESSIONS_DIR_ENV).ok();
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, base.to_str().unwrap());
    set_tddy_data_dir_override(Some(base.clone()));

    let session_id = "00000000-0007-0000-0000-000000000030";
    let session_dir = create_session_dir_with_id(&base, session_id).unwrap();
    let unique_before = "UNIQUE_TOKEN_ALPHA_7f3c";
    write_minimal_changeset(
        &session_dir,
        session_id,
        "initial prompt without target token",
        Some("/wt/before"),
        Some("feature/before"),
        None,
        None,
    );

    index_session_for_search(&base, session_id, &session_dir).unwrap();

    assert!(
        search_sessions_semantic(&base, unique_before)
            .unwrap()
            .is_empty(),
        "before update, unique token must not appear in search results"
    );

    let unique_after = "UNIQUE_TOKEN_BETA_9d1a";
    write_minimal_changeset(
        &session_dir,
        session_id,
        unique_after,
        Some("/wt/after"),
        Some("feature/after"),
        None,
        None,
    );
    index_session_for_search(&base, session_id, &session_dir).unwrap();

    let hits = search_sessions_semantic(&base, unique_after).expect("search after update");
    assert_eq!(
        hits.len(),
        1,
        "after changeset update and re-index, search must find the session by the new unique token"
    );
    assert_eq!(hits[0].session_id, session_id);

    if let Some(p) = prev {
        std::env::set_var(TDDY_SESSIONS_DIR_ENV, p);
    } else {
        std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    }
    set_tddy_data_dir_override(None);
}
