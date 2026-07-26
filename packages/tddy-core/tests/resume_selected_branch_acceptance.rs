//! Acceptance: resuming an existing branch checks out that branch, and records it.
//!
//! Recovering a planned PR whose child session was deleted means starting a session on the branch the
//! node already owns. The branch picker (`ListProjectBranches`) offers *remote-tracking* names
//! (`origin/feature/x` — `list_recent_remote_branches` reads `refs/remotes/origin`), so
//! `selected_branch_to_work_on` arrives in that form. `git worktree add <path> origin/feature/x`
//! succeeds with a **detached HEAD**: the session appears healthy while every commit it makes is
//! unreachable and there is nothing to push. Only the unprefixed name makes git create (or reuse) the
//! local branch that tracks `origin/feature/x`.
//!
//! `Changeset.branch` is the field the whole PR-stack recovery is keyed on — `QueryBranch` scans
//! sessions by it, and a stack node is linked by it — so it has to hold the local branch name, which
//! is also what [`tddy_core::worktree::local_branch_name`] resolves.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C1, D2, D3).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_core::changeset::BranchWorktreeIntent;
use tddy_core::changeset::{read_changeset, write_changeset, Changeset, ChangesetWorkflow};
use tddy_core::worktree::setup_worktree_for_session_with_integration_base;

fn scratch(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "tddy-resume-selected-branch-{}-{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repo whose `origin` carries `master` plus `branch`, while the local checkout has **no** local
/// branch for it — the state a host is in after the session that created the branch was deleted, or
/// when the branch was pushed from another machine.
fn a_repo_whose_origin_owns(base: &Path, branch: &str) -> PathBuf {
    let origin = base.join("origin.git");
    git(base, &["init", "--bare", origin.to_str().unwrap()]);

    let repo = base.join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=master"]);
    git(&repo, &["config", "user.email", "operator@example.com"]);
    git(&repo, &["config", "user.name", "Operator"]);
    fs::write(repo.join("README.md"), "stack\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-m", "initial"]);
    git(
        &repo,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(&repo, &["push", "-u", "origin", "master"]);

    git(&repo, &["checkout", "-b", branch]);
    fs::write(repo.join("store.rs"), "pub struct Store;\n").unwrap();
    git(&repo, &["add", "store.rs"]);
    git(&repo, &["commit", "-m", "attach store"]);
    git(&repo, &["push", "-u", "origin", branch]);
    git(&repo, &["checkout", "master"]);
    git(&repo, &["branch", "-D", branch]);
    repo
}

/// A session that was asked to work on `selected_branch`, with nothing else to derive a name from.
fn a_session_resuming(base: &Path, selected_branch: &str) -> PathBuf {
    let session_dir = base.join("session");
    fs::create_dir_all(&session_dir).unwrap();
    write_changeset(
        &session_dir,
        &Changeset {
            name: Some("Session attachment storage".into()),
            workflow: Some(ChangesetWorkflow {
                branch_worktree_intent: Some(BranchWorktreeIntent::WorkOnSelectedBranch),
                selected_branch_to_work_on: Some(selected_branch.to_string()),
                ..ChangesetWorkflow::default()
            }),
            ..Changeset::default()
        },
    )
    .unwrap();
    session_dir
}

/// The branch checked out in `worktree`, or `None` when HEAD is detached.
fn checked_out_branch(worktree: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .current_dir(worktree)
        .output()
        .expect("git symbolic-ref");
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

const OWNED_BRANCH: &str = "feature/attach-docs/attach-store";

#[test]
fn resuming_a_remote_tracking_branch_checks_out_its_local_branch_rather_than_detaching_head() {
    // Given — the node's branch exists only on origin; the session is asked to resume it by the
    // remote-tracking name the branch picker offers
    let base = scratch("checks-out");
    let repo = a_repo_whose_origin_owns(&base, OWNED_BRANCH);
    let session_dir = a_session_resuming(&base, &format!("origin/{OWNED_BRANCH}"));

    // When
    let worktree =
        setup_worktree_for_session_with_integration_base(&repo, &session_dir, "origin/master")
            .expect("resuming a pushed branch must create a worktree");

    // Then — a detached HEAD would make every commit the session produces unreachable
    let branch = checked_out_branch(&worktree);
    let _ = fs::remove_dir_all(&base);
    assert_eq!(branch.as_deref(), Some(OWNED_BRANCH));
}

#[test]
fn resuming_a_remote_tracking_branch_records_the_local_branch_on_the_changeset() {
    // Given — the same recovery, whose durability depends on `Changeset.branch`: `QueryBranch` scans
    // sessions by it and the orchestrator's stack node is linked by it
    let base = scratch("records");
    let repo = a_repo_whose_origin_owns(&base, OWNED_BRANCH);
    let session_dir = a_session_resuming(&base, &format!("origin/{OWNED_BRANCH}"));

    // When
    setup_worktree_for_session_with_integration_base(&repo, &session_dir, "origin/master")
        .expect("resuming a pushed branch must create a worktree");

    // Then — the branch the node owns, so the resumed session resolves back to that node
    let recorded = read_changeset(&session_dir).expect("changeset").branch;
    let _ = fs::remove_dir_all(&base);
    assert_eq!(recorded.as_deref(), Some(OWNED_BRANCH));
}

#[test]
fn resuming_an_unprefixed_branch_name_records_that_same_branch() {
    // Given — a client that already sends the local branch name (the daemon's own vocabulary)
    let base = scratch("unprefixed");
    let repo = a_repo_whose_origin_owns(&base, OWNED_BRANCH);
    let session_dir = a_session_resuming(&base, OWNED_BRANCH);

    // When
    setup_worktree_for_session_with_integration_base(&repo, &session_dir, "origin/master")
        .expect("resuming a pushed branch must create a worktree");

    // Then — normalizing the remote-tracking form must not disturb the already-local form
    let recorded = read_changeset(&session_dir).expect("changeset").branch;
    let _ = fs::remove_dir_all(&base);
    assert_eq!(recorded.as_deref(), Some(OWNED_BRANCH));
}
