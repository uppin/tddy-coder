//! What `#carve` 6/11 delivers: git plumbing in `tddy-git`, and the GitHub REST client in the crate
//! that was always meant to own it.
//!
//! Two bodies of low-level plumbing sit in crates that have nothing to do with them, and `#carve`
//! 10/11 consumes both. `worktree.rs` is 1,606 production lines of `git` wrappers with **four**
//! session-aware functions bolted on — `setup_worktree_for_session`, its `_with_integration_base`
//! and `_with_optional_chain_base` variants, and
//! `resolve_persisted_worktree_integration_base_for_session` — and nothing else in the file names a
//! `tddy-core` symbol. Meanwhile 2,124 lines of GitHub REST live in a workflow-recipes crate while
//! `tddy-github` has no PR surface at all.
//!
//! These assertions read manifests and source, because "which crate owns this" is not a question the
//! type system answers once everything compiles.

use std::path::{Path, PathBuf};

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(name)
}

/// A manifest, or empty when the crate does not exist — for asserting that it exists.
fn manifest(name: &str) -> String {
    std::fs::read_to_string(package(name).join("Cargo.toml")).unwrap_or_default()
}

/// A manifest that must exist — for asserting what it does not contain, which an empty string
/// would satisfy vacuously.
fn required_manifest(name: &str) -> String {
    std::fs::read_to_string(package(name).join("Cargo.toml"))
        .unwrap_or_else(|err| panic!("`packages/{name}/Cargo.toml` could not be read: {err}"))
}

/// The modules a crate root declares: lines that, trimmed, are exactly `pub mod <name>;`.
fn declared_modules(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub mod "))
        .filter_map(|rest| rest.strip_suffix(';'))
        .map(str::to_string)
        .collect()
}

/// Everything before the first `#[cfg(test)]`.
fn production(text: &str) -> String {
    match text
        .lines()
        .position(|line| line.trim_start().starts_with("#[cfg(test)]"))
    {
        Some(at) => text.lines().take(at).collect::<Vec<_>>().join("\n"),
        None => text.to_string(),
    }
}

/// AC1 — `tddy-git` exists and depends on no workspace crate.
///
/// Plumbing that depends on nothing is plumbing anything can use. A `tddy-git` that reached back
/// into `tddy-core` would have moved the lines without moving the coupling.
#[test]
fn tddy_git_is_a_crate_that_depends_on_no_other() {
    // Given the new crate's manifest
    let text = manifest("tddy-git");

    // Then it exists
    assert!(
        !text.is_empty(),
        "`packages/tddy-git` has no manifest — the crate does not exist yet"
    );

    // And names no workspace crate
    let workspace_deps: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("tddy-"))
        .collect();
    assert!(
        workspace_deps.is_empty(),
        "`tddy-git` depends on workspace crates: {workspace_deps:?}"
    );
}

/// AC1 — nothing in `tddy-git` knows what a session is.
///
/// The seam is exact: four functions stay behind, and `worktree.rs` drops from 1,606 production lines
/// to 428.
#[test]
fn tddy_git_holds_nothing_session_aware() {
    // Given every source file of the new crate
    let source = package("tddy-git").join("src");
    assert!(
        source.exists(),
        "`packages/tddy-git/src` does not exist yet"
    );

    // When each is searched for the session model
    let reaching: Vec<String> = std::fs::read_dir(&source)
        .expect("the source directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
            production(&text).contains("Changeset")
        })
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();

    // Then none of them does
    assert!(
        reaching.is_empty(),
        "these name the session model and belong in `tddy-core`: {reaching:?}"
    );
}

fn core_worktree_source() -> String {
    std::fs::read_to_string(package("tddy-core").join("src/worktree.rs"))
        .expect("tddy-core/src/worktree.rs")
}

/// AC2 — the plumbing left `tddy-core::worktree`, bringing it under its line budget.
#[test]
fn the_core_worktree_module_shrinks_below_its_line_budget() {
    // Given the module after the split
    let text = core_worktree_source();

    // When its production lines are counted
    let lines = production(&text).lines().count();

    // Then it is under budget (a budget, so a threshold rather than an exact count)
    assert!(
        lines < 450,
        "`worktree.rs` is still {lines} production lines, so the plumbing did not leave"
    );
}

/// AC2 — the session-aware layer stays in `tddy-core::worktree`.
#[test]
fn the_core_worktree_module_keeps_the_session_aware_layer() {
    // Given the module after the split
    let text = core_worktree_source();

    // When its production part is taken
    let production = production(&text);

    // Then the session-aware layer is still there
    assert!(
        production.contains("setup_worktree_for_session"),
        "the session-aware layer was moved out; it belongs in `tddy-core`"
    );
}

/// AC4 — `tddy-github` owns the PR REST surface.
///
/// Three modules moved: `github_pr`, `github_rest_common`, and `pr_api` — the renamed
/// `orchestrate_pr_stack/github.rs`. Only a `pub mod` declaration counts; a comment naming the
/// module does not.
#[test]
fn tddy_github_owns_the_pull_request_surface() {
    // Given the crate's own module root
    let lib = std::fs::read_to_string(package("tddy-github").join("src/lib.rs"))
        .expect("tddy-github/src/lib.rs");

    // When its module declarations are read
    let declared = declared_modules(&lib);
    let missing: Vec<&str> = ["github_pr", "github_rest_common", "pr_api"]
        .into_iter()
        .filter(|module| !declared.iter().any(|name| name == module))
        .collect();

    // Then it publishes all three moved modules
    assert!(
        missing.is_empty(),
        "`tddy-github` does not publish these, so the REST client has not moved: {missing:?}"
    );
}

/// AC6 — the move adds no dependency that could close a cycle.
///
/// The origin crate is the one that can close a cycle: it keeps facades at the old paths, so it
/// depends on `tddy-github`. An edge back the other way would make the pair mutually dependent.
///
/// `tddy-core` is deliberately **not** on this list. `orchestrate_pr_stack/github.rs` states
/// `tddy_core::WorkflowError` in sixteen production signatures, including every method of the
/// public `GithubPrApi` trait, so the client cannot move without that edge. It closes no cycle:
/// `tddy-core` does not depend on `tddy-github`, directly or transitively.
#[test]
fn tddy_github_gains_no_dependency_on_the_crate_the_client_left() {
    // Given its manifest
    let text = required_manifest("tddy-github");

    // Then it does not depend on the crate the client left
    assert!(
        !text.contains("tddy-workflow-recipes"),
        "`tddy-github` gained a dependency on `tddy-workflow-recipes`, which holds the facades \
         pointing here — the two crates would depend on each other"
    );
}
