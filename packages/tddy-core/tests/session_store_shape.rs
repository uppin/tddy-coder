//! What `#carve` 6/9 delivers: the session storage layer in its own crates, and `sqlx` out of the
//! god-crate's dependency tree.
//!
//! **35 crates depend on `tddy-core`, and every one of them compiles `sqlx` with bundled SQLite.**
//! It is named by exactly four files, all under `session_catalog/`, and nothing else in `tddy-core`
//! touches a database.
//!
//! The group hinges on one DTO. `session_catalog` needs `session_actions`, which needs `output` and
//! `atomic_file`, which need `error` — and `error.rs` has exactly one edge out of the group:
//! `use crate::backend::ClarificationQuestion;`, for one `WorkflowError` variant. `#carve` 4/9 moves
//! that DTO, and the group becomes a closed DAG.

use std::path::{Path, PathBuf};

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(name)
}

fn manifest(name: &str) -> String {
    std::fs::read_to_string(package(name).join("Cargo.toml")).unwrap_or_default()
}

/// AC1 — `tddy-core` names neither `sqlx` nor SQLite.
///
/// This is the whole point of the node. The manifest alone does not prove the win — a path
/// dependency can re-introduce it, which is what AC2's `cargo tree` covers at `/green` — but a
/// manifest that still declares it proves the loss.
#[test]
fn the_god_crate_no_longer_declares_a_database() {
    // Given its manifest
    let text = manifest("tddy-core");

    // Then neither the driver nor the engine is in it
    assert!(
        !text.contains("sqlx"),
        "`tddy-core` still declares `sqlx`, so all 35 of its dependents still compile SQLite"
    );
}

/// AC1 — and it keeps `jsonschema`, which does **not** leave.
///
/// `session_actions/validate.rs` moves, but `session_action_pipeline.rs` stays and still names it.
/// Claiming otherwise would be wrong, and an earlier note in this stack did.
#[test]
fn the_god_crate_keeps_the_dependency_that_does_not_leave() {
    // Given its manifest
    let text = manifest("tddy-core");

    // Then `jsonschema` is still there, because a module that stays still needs it
    assert!(
        text.contains("jsonschema"),
        "`jsonschema` was removed, but `session_action_pipeline.rs` stays in `tddy-core` and names it"
    );
}

/// The only workspace crates `tddy-session-store` may depend on. None of them depends on
/// `tddy-core`, so none can close a cycle back into the crate the storage layer left.
const STORAGE_CRATE_WORKSPACE_DEPENDENCIES: [&str; 3] =
    ["tddy-workflow", "tddy-actions", "tddy-task"];

/// AC3 — `tddy-session-store` depends on the vocabulary and the action runtime, and nothing else of
/// ours.
///
/// `tddy-workflow` is where `#carve` 4/9 puts `ClarificationQuestion`, which `error.rs` names.
/// `session_actions/runtime.rs` runs every manifest on the action runtime (`tddy-actions`) and
/// tracks it in the task registry (`tddy-task`). Any other workspace dependency means the seam was
/// cut in the wrong place.
#[test]
fn the_storage_crate_depends_only_on_the_vocabulary() {
    // Given the new crate's manifest
    let text = manifest("tddy-session-store");
    assert!(
        !text.is_empty(),
        "`packages/tddy-session-store` has no manifest — the crate does not exist yet"
    );

    // When its workspace dependencies are listed
    let unexpected: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("tddy-"))
        .filter(|line| !STORAGE_CRATE_WORKSPACE_DEPENDENCIES.contains(&dependency_name(line)))
        .collect();

    // Then only the allowlisted crates are among them
    assert!(
        unexpected.is_empty(),
        "`tddy-session-store` depends on workspace crates outside \
         {STORAGE_CRATE_WORKSPACE_DEPENDENCIES:?}: {unexpected:?}"
    );
}

/// The crate name a manifest dependency line declares: `tddy-task = { … }` → `tddy-task`.
fn dependency_name(line: &str) -> &str {
    line.split('=').next().unwrap_or(line).trim()
}

/// AC4 — the catalog crate takes `sqlx` with it, and depends back on nothing.
#[test]
fn the_catalog_crate_owns_the_database_and_depends_on_the_store() {
    // Given the new crate's manifest
    let text = manifest("tddy-session-catalog");
    assert!(
        !text.is_empty(),
        "`packages/tddy-session-catalog` has no manifest — the crate does not exist yet"
    );

    // Then it owns the database
    assert!(
        text.contains("sqlx"),
        "`tddy-session-catalog` does not declare `sqlx` — the dependency went somewhere else"
    );

    // And it does not depend back on the crate it left
    assert!(
        !text.contains("tddy-core"),
        "`tddy-session-catalog` depends on `tddy-core`, which is the edge this node removes"
    );
}
