//! Unit tests for the unified chain base resolution moved to tddy-core.
//!
//! These tests define the API contract for `resolve_chain_base_ref` and
//! `resolve_chain_base_for_session_spawn` in `tddy-core::session_chain`. They
//! fail (red) until the functions are moved from `tddy-daemon::connection_service`
//! and the new precedence-aware wrapper is added.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_core::changeset::{Stack, StackNode};
use tddy_core::session_chain::{resolve_chain_base_for_session_spawn, resolve_chain_base_ref};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{write_changeset, Changeset};

fn scratch(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "tddy-unified-chain-{}-{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn git(repo: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&o.stderr)
    );
}

fn init_repo_with_origin_master(repo: &Path) {
    fs::create_dir_all(repo).unwrap();
    git(repo, &["init"]);
    git(repo, &["config", "user.email", "test@test.com"]);
    git(repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("README"), "initial").unwrap();
    git(repo, &["add", "README"]);
    git(repo, &["commit", "-m", "initial"]);
    git(repo, &["branch", "-M", "master"]);
    git(repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
    git(repo, &["push", "-u", "origin", "master"]);
}

fn push_branch(repo: &Path, branch: &str) {
    git(repo, &["checkout", "-b", branch, "origin/master"]);
    git(
        repo,
        &["commit", "--allow-empty", "-m", &format!("on {branch}")],
    );
    git(repo, &["push", "-u", "origin", branch]);
    git(repo, &["checkout", "master"]);
}

fn write_parent_changeset(sessions_base: &Path, session_id: &str, cs: Changeset) {
    let dir = unified_session_dir_path(sessions_base, session_id);
    fs::create_dir_all(&dir).expect("create parent session dir");
    write_changeset(&dir, &cs).expect("write parent changeset");
}

fn a_materialized_node(node_id: &str, branch: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: node_id.to_string(),
        description: String::new(),
        branch_suggestion: Some(branch.to_string()),
        branch: Some(branch.to_string()),
        session_id: None,
        parents: parents.iter().map(|p| p.to_string()).collect(),
        pr_status: None,
        child_state: None,
        internal_status: None,
        display_order: None,
    }
}

fn a_planned_child_node(node_id: &str, branch_suggestion: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: node_id.to_string(),
        description: String::new(),
        branch_suggestion: Some(branch_suggestion.to_string()),
        branch: None,
        session_id: None,
        parents: parents.iter().map(|p| p.to_string()).collect(),
        pr_status: None,
        child_state: None,
        internal_status: None,
        display_order: None,
    }
}

/// **returns_stack_node_base_for_pr_stack_orchestrator_parent** — When the
/// parent is a pr-stack orchestrator and the new branch matches a planned node,
/// `resolve_chain_base_ref` returns that node's nearest non-merged ancestor's
/// `origin/<branch>`.
#[test]
fn resolve_chain_base_ref_returns_stack_node_base_for_pr_stack_orchestrator_parent() {
    // Given
    let base = scratch("orchestrator-base");
    let repo = base.join("repo");
    init_repo_with_origin_master(&repo);
    let parent_branch = "feature/stack/parent";
    let child_branch = "feature/stack/child";
    push_branch(&repo, parent_branch);

    let sessions_base = base.join("sessions");
    let orchestrator_id = "orchestrator-1";
    write_parent_changeset(
        &sessions_base,
        orchestrator_id,
        Changeset {
            recipe: Some("pr-stack".to_string()),
            stack: Some(Stack {
                version: 1,
                nodes: vec![
                    a_materialized_node("bottom", parent_branch, &[]),
                    a_planned_child_node("child", child_branch, &["bottom"]),
                ],
            }),
            ..Changeset::default()
        },
    );

    // When
    let result = resolve_chain_base_ref(&sessions_base, Some(orchestrator_id), &repo, child_branch);

    // Then
    assert_eq!(
        result.expect("pr-stack orchestrator parent must resolve"),
        Some(format!("origin/{parent_branch}")),
        "must return the planned node's parent branch, not None"
    );

    let _ = fs::remove_dir_all(&base);
}

/// **returns_none_for_branchless_pr_stack_orchestrator** — A pr-stack
/// orchestrator with no matching node yields `Ok(None)` (default base), not an
/// error.
#[test]
fn resolve_chain_base_ref_returns_none_for_branchless_pr_stack_orchestrator() {
    // Given
    let base = scratch("branchless-orch");
    let repo = base.join("repo");
    init_repo_with_origin_master(&repo);

    let sessions_base = base.join("sessions");
    write_parent_changeset(
        &sessions_base,
        "orchestrator-1",
        Changeset {
            recipe: Some("pr-stack".to_string()),
            stack: Some(Stack {
                version: 1,
                nodes: vec![],
            }),
            ..Changeset::default()
        },
    );

    // When
    let result = resolve_chain_base_ref(
        &sessions_base,
        Some("orchestrator-1"),
        &repo,
        "feature/unmatched",
    );

    // Then
    assert_eq!(
        result.expect("branchless orchestrator must resolve, not error"),
        None,
        "a pr-stack orchestrator with no matching node must yield None (default base)"
    );

    let _ = fs::remove_dir_all(&base);
}

/// **errors_for_branchless_code_session_parent** — A regular code-session
/// parent with no branch errors (ported from daemon).
#[test]
fn resolve_chain_base_ref_errors_for_branchless_code_session_parent() {
    // Given
    let base = scratch("branchless-code");
    let repo = base.join("repo");
    init_repo_with_origin_master(&repo);

    let sessions_base = base.join("sessions");
    write_parent_changeset(
        &sessions_base,
        "code-session-1",
        Changeset {
            recipe: Some("tdd".to_string()),
            stack: None,
            branch: None,
            ..Changeset::default()
        },
    );

    // When
    let result = resolve_chain_base_ref(
        &sessions_base,
        Some("code-session-1"),
        &repo,
        "feature/some-branch",
    );

    // Then
    let err = result.expect_err("a branchless code-session parent must error");
    assert!(
        err.to_string()
            .contains("could not resolve stack parent branch")
            || err.to_string().contains("branch"),
        "error must mention the branch problem: {}",
        err
    );

    let _ = fs::remove_dir_all(&base);
}

/// **spawn_returns_none_when_no_stack_parent_and_no_persisted_field** —
/// `resolve_chain_base_for_session_spawn` with no stack parent and no
/// persisted field returns `Ok(None)` (default base).
#[test]
fn spawn_returns_none_when_no_stack_parent_and_no_persisted_field() {
    // Given
    let base = scratch("spawn-none");
    let repo = base.join("repo");
    init_repo_with_origin_master(&repo);

    // When
    let result = resolve_chain_base_for_session_spawn(
        &base.join("sessions"),
        None,
        &repo,
        "feature/any",
        None,
    );

    // Then
    assert_eq!(
        result.expect("no stack parent and no persisted field must resolve to None"),
        None,
        "absence of both stack_parent and persisted field must yield None (default base)"
    );

    let _ = fs::remove_dir_all(&base);
}

/// **spawn_returns_persisted_field_when_no_stack_parent** — When a persisted
/// `worktree_integration_base_ref` is present and no stack_parent, the
/// persisted field wins (Telegram / workflow-recipe compatibility).
#[test]
fn spawn_returns_persisted_field_when_no_stack_parent() {
    // Given
    let base = scratch("spawn-persisted");
    let repo = base.join("repo");
    init_repo_with_origin_master(&repo);
    push_branch(&repo, "feature/persisted");

    // When
    let result = resolve_chain_base_for_session_spawn(
        &base.join("sessions"),
        None,
        &repo,
        "feature/any",
        Some("origin/feature/persisted"),
    );

    // Then
    assert_eq!(
        result.expect("persisted field with no stack parent must resolve"),
        Some("origin/feature/persisted".to_string()),
        "persisted field must be honored when no stack_parent is supplied"
    );

    let _ = fs::remove_dir_all(&base);
}

/// **spawn_stack_parent_takes_precedence_over_persisted_field** — When both a
/// stack_parent and a persisted field are present, the stack_parent resolution
/// wins (it's the freshest runtime request).
#[test]
fn spawn_stack_parent_takes_precedence_over_persisted_field() {
    // Given
    let base = scratch("spawn-precedence");
    let repo = base.join("repo");
    init_repo_with_origin_master(&repo);
    let parent_branch = "feature/stack/parent";
    let child_branch = "feature/stack/child";
    push_branch(&repo, parent_branch);

    let sessions_base = base.join("sessions");
    let orchestrator_id = "orchestrator-1";
    write_parent_changeset(
        &sessions_base,
        orchestrator_id,
        Changeset {
            recipe: Some("pr-stack".to_string()),
            stack: Some(Stack {
                version: 1,
                nodes: vec![
                    a_materialized_node("bottom", parent_branch, &[]),
                    a_planned_child_node("child", child_branch, &["bottom"]),
                ],
            }),
            ..Changeset::default()
        },
    );

    // When — both stack_parent and a persisted field are supplied.
    let result = resolve_chain_base_for_session_spawn(
        &sessions_base,
        Some(orchestrator_id),
        &repo,
        child_branch,
        Some("origin/feature/stale-persisted"),
    );

    // Then — stack_parent wins.
    assert_eq!(
        result.expect("both inputs must resolve"),
        Some(format!("origin/{parent_branch}")),
        "stack_parent resolution must take precedence over the persisted field"
    );

    let _ = fs::remove_dir_all(&base);
}
