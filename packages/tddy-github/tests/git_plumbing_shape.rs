//! What `#carve` 5/9 delivers: git plumbing in `tddy-git`, and the GitHub REST client in the crate
//! that was always meant to own it.
//!
//! Two bodies of low-level plumbing sit in crates that have nothing to do with them, and `#carve`
//! 9/9 consumes both. `worktree.rs` is 1,606 production lines of `git` wrappers with **two**
//! session-aware functions bolted on — its only `crate::changeset` import is consumed inside
//! `setup_worktree_for_session_with_integration_base` and
//! `setup_worktree_for_session_with_optional_chain_base`, and nothing else in the file names a
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

fn manifest(name: &str) -> String {
    std::fs::read_to_string(package(name).join("Cargo.toml")).unwrap_or_default()
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
/// The seam is exact: two functions stay behind, ~1,200 lines leave.
#[test]
fn tddy_git_holds_nothing_session_aware() {
    // Given every source file of the new crate
    let source = package("tddy-git").join("src");
    if !source.exists() {
        panic!("`packages/tddy-git/src` does not exist yet");
    }

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

/// AC2 — `tddy-core::worktree` keeps only the session-aware layer, behind a facade.
#[test]
fn the_core_worktree_module_keeps_only_what_knows_about_sessions() {
    // Given the module after the split
    let text = std::fs::read_to_string(package("tddy-core").join("src/worktree.rs"))
        .expect("tddy-core/src/worktree.rs");
    let lines = production(&text).lines().count();

    // Then what is left is the session-aware layer and a facade
    assert!(
        lines < 450,
        "`worktree.rs` is still {lines} production lines, so the plumbing did not leave"
    );
    assert!(
        production(&text).contains("setup_worktree_for_session"),
        "the session-aware layer was moved out; it belongs in `tddy-core`"
    );
}

/// AC4 — `tddy-github` owns the PR REST surface.
#[test]
fn tddy_github_owns_the_pull_request_surface() {
    // Given the crate's own module root
    let lib = std::fs::read_to_string(package("tddy-github").join("src/lib.rs"))
        .expect("tddy-github/src/lib.rs");

    // Then it publishes the three moved modules
    for module in ["github_pr", "github_rest_common"] {
        assert!(
            lib.contains(module),
            "`tddy-github` does not publish `{module}` — the REST client has not moved"
        );
    }
}

/// AC6 — the move adds no dependency that could close a cycle.
///
/// `tddy-github` depends only on `tddy-rpc` and `tddy-service`, and neither the origin crate nor
/// `tddy-core` may join them: the three moved files name nothing from either.
#[test]
fn tddy_github_gains_no_dependency_on_the_crate_the_client_left() {
    // Given its manifest
    let text = manifest("tddy-github");

    // Then it does not depend on the origin, nor on the god-crate
    for forbidden in ["tddy-workflow-recipes", "tddy-core"] {
        assert!(
            !text.contains(forbidden),
            "`tddy-github` gained a dependency on `{forbidden}`, which the moved files do not need"
        );
    }
}
