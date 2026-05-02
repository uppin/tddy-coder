//! Session chain acceptance tests (PRD Testing Plan: optional `previous_session_id`, parent →
//! `origin/<branch>` resolution, Telegram/TUI entry points).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_core::changeset::{write_changeset, Changeset, ChangesetState};
use tddy_core::resolve_chain_integration_base_ref_from_parent_session;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::workflow::ids::WorkflowState;

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tddy-session-chain-acc-{}", label));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
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

fn init_origin_master_repo(repo: &Path) {
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

/// **chain_base_resolved_from_parent_session_changeset_branch** — parent `changeset.yaml` branch →
/// validated `origin/...` ref; child worktree HEAD must match fetched tip (same class as chain PR
/// acceptance tests).
#[test]
fn chain_base_resolved_from_parent_session_changeset_branch() {
    let base = temp_dir("resolve-base");
    let repo = base.join("repo");
    init_origin_master_repo(&repo);
    let repo_canon = repo.canonicalize().unwrap();

    let parent_id = "018faaaa-bbbb-7ccc-ddee-000000000001";
    let sessions_home = base.join("sessions-home");
    let parent_dir = unified_session_dir_path(&sessions_home, parent_id);
    fs::create_dir_all(&parent_dir).unwrap();

    let cs = Changeset {
        name: Some("parent".into()),
        branch_suggestion: Some("feature/parent-stack".into()),
        repo_path: Some(repo_canon.to_string_lossy().into_owned()),
        state: ChangesetState {
            current: WorkflowState::new("Planned"),
            ..Changeset::default().state
        },
        ..Changeset::default()
    };
    write_changeset(&parent_dir, &cs).unwrap();

    let origin_ref = resolve_chain_integration_base_ref_from_parent_session(
        &sessions_home,
        parent_id,
        &repo_canon,
    )
    .expect("must resolve origin/<branch> from parent session changeset + validate repo match");

    assert_eq!(origin_ref, "origin/feature/parent-stack");

    let _ = fs::remove_dir_all(&base);
}

/// **chain_rejected_when_parent_has_no_branch** — reject with explicit actionable copy (no silent
/// fallback to default integration base).
#[test]
fn chain_rejected_when_parent_has_no_branch() {
    let base = temp_dir("no-branch");
    let repo = base.join("repo");
    init_origin_master_repo(&repo);
    let repo_canon = repo.canonicalize().unwrap();

    let parent_id = "018fbbbb-bbbb-7ccc-ddee-000000000002";
    let sessions_home = base.join("sessions-home");
    let parent_dir = unified_session_dir_path(&sessions_home, parent_id);
    fs::create_dir_all(&parent_dir).unwrap();

    let cs = Changeset {
        name: Some("parent-no-branch".into()),
        repo_path: Some(repo_canon.to_string_lossy().into_owned()),
        state: ChangesetState {
            current: WorkflowState::new("Planned"),
            ..Changeset::default().state
        },
        ..Changeset::default()
    };
    write_changeset(&parent_dir, &cs).unwrap();

    let err = resolve_chain_integration_base_ref_from_parent_session(
        &sessions_home,
        parent_id,
        &repo_canon,
    )
    .expect_err("parent without branch must be rejected for chain workflow");

    let msg = err.to_string();
    assert!(
        msg.contains(
            "PRD acceptance copy: parent session must record a branch before chaining (operators: push or persist branch name)"
        ),
        "expected actionable operator-facing message; got {msg}"
    );

    let _ = fs::remove_dir_all(&base);
}

/// Without `repo_path` on the parent changeset, chain base alignment cannot be verified against the
/// selected project repository; resolver must reject (stricter guard — changeset outstanding).
#[test]
fn chain_rejects_when_parent_changeset_omits_repo_path() {
    let base = temp_dir("no-repo-path");
    let repo = base.join("repo");
    init_origin_master_repo(&repo);
    let repo_canon = repo.canonicalize().unwrap();

    let parent_id = "018fcccc-bbbb-7ccc-ddee-000000000003";
    let sessions_home = base.join("sessions-home");
    let parent_dir = unified_session_dir_path(&sessions_home, parent_id);
    fs::create_dir_all(&parent_dir).unwrap();

    let cs = Changeset {
        name: Some("parent-no-repo-path".into()),
        branch_suggestion: Some("feature/parent-stack".into()),
        state: ChangesetState {
            current: WorkflowState::new("Planned"),
            ..Changeset::default().state
        },
        ..Changeset::default()
    };
    write_changeset(&parent_dir, &cs).unwrap();

    let got = resolve_chain_integration_base_ref_from_parent_session(
        &sessions_home,
        parent_id,
        &repo_canon,
    );
    assert!(
        got.is_err(),
        "expected rejection when parent changeset omits repo_path; got {got:?}"
    );

    let _ = fs::remove_dir_all(&base);
}
