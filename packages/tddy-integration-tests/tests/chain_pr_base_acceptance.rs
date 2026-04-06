//! Acceptance tests for chain PRs: optional base branch when starting a workflow (PRD Testing Plan).
//!
//! These tests are expected to fail (RED) until:
//! - `changeset.yaml` persists effective and user-selected integration base refs,
//! - `setup_worktree_for_session_with_optional_chain_base` implements `Some(origin/...)` paths,
//! - `validate_chain_pr_integration_base_ref` accepts safe multi-segment refs,
//! - resume reads the persisted base via `resolve_persisted_worktree_integration_base_for_session`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_core::changeset::{read_changeset, write_changeset, Changeset, ChangesetState};
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::{
    resolve_default_integration_base_ref, resolve_persisted_worktree_integration_base_for_session,
    setup_worktree_for_session_with_optional_chain_base, validate_chain_pr_integration_base_ref,
};

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tddy-chain-pr-acc-{}", label));
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

/// Single-branch remote `origin/master` fixture (same shape as existing worktree acceptance tests).
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

/// `origin/master` plus `origin/feature/pr-base` one commit ahead (for merge-base assertions).
fn init_origin_master_and_feature_pr_base(repo: &Path) {
    init_origin_master_repo(repo);
    git(repo, &["checkout", "-b", "feature/pr-base"]);
    fs::write(repo.join("chain.txt"), "pr-base work\n").unwrap();
    git(repo, &["add", "chain.txt"]);
    git(repo, &["commit", "-m", "on pr-base"]);
    git(repo, &["push", "-u", "origin", "feature/pr-base"]);
    git(repo, &["checkout", "master"]);
}

fn session_with_suggestions(name: &str, branch: &str, worktree: &str) -> Changeset {
    Changeset {
        name: Some(name.to_string()),
        initial_prompt: Some("chain pr".to_string()),
        state: ChangesetState {
            current: WorkflowState::new("Planned"),
            ..Changeset::default().state
        },
        branch_suggestion: Some(branch.to_string()),
        worktree_suggestion: Some(worktree.to_string()),
        ..Changeset::default()
    }
}

fn merge_base_is_ancestor(repo: &Path, tip: &str, ancestor: &str) -> bool {
    let o = Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, tip])
        .current_dir(repo)
        .status()
        .unwrap();
    o.success()
}

/// **worktree_defaults_unchanged_when_base_not_selected** — With no optional base, effective
/// integration base matches default resolution; once implemented, `changeset.yaml` must record the
/// effective ref for observability (same string as `resolve_default_integration_base_ref`).
#[test]
fn chain_pr_worktree_defaults_unchanged_when_base_not_selected() {
    let base = temp_dir("defaults-unchanged");
    let repo = base.join("repo");
    init_origin_master_repo(&repo);

    let session_dir = base.join("plan");
    fs::create_dir_all(&session_dir).unwrap();
    let cs = session_with_suggestions("Chain Default", "feature/chain-def", "feature-chain-def");
    write_changeset(&session_dir, &cs).unwrap();
    fs::write(session_dir.join("PRD.md"), "# PRD\n## TODO\n- [ ] x\n").unwrap();

    let expected_base = resolve_default_integration_base_ref(&repo).unwrap();

    let path_optional =
        setup_worktree_for_session_with_optional_chain_base(&repo, &session_dir, None).unwrap();
    let head_opt = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&path_optional)
        .output()
        .unwrap();
    let base_rev = Command::new("git")
        .args(["rev-parse", &expected_base])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&head_opt.stdout).trim(),
        String::from_utf8_lossy(&base_rev.stdout).trim(),
        "optional None path must match default integration base tip"
    );

    let yaml = fs::read_to_string(session_dir.join("changeset.yaml")).unwrap();
    assert!(
        yaml.contains("effective_worktree_integration_base_ref:")
            && yaml.contains(&expected_base),
        "PRD observability: changeset must persist effective integration base ref; expected fragment for {:?} in:\n{}",
        expected_base,
        yaml
    );

    let _ = fs::remove_dir_all(&base);
}

/// **worktree_branches_from_selected_origin_ref** — New worktree branch tip is an ancestor of
/// the chosen `origin/...` ref (here `origin/feature/pr-base`), not only `origin/master`.
#[test]
fn chain_pr_worktree_branches_from_selected_origin_ref() {
    let base = temp_dir("branch-from-selected");
    let repo = base.join("repo");
    init_origin_master_and_feature_pr_base(&repo);

    let session_dir = base.join("plan");
    fs::create_dir_all(&session_dir).unwrap();
    let cs = session_with_suggestions("Chain Child", "feature/chain-child", "feature-chain-child");
    write_changeset(&session_dir, &cs).unwrap();
    fs::write(session_dir.join("PRD.md"), "# PRD\n").unwrap();

    let wt = setup_worktree_for_session_with_optional_chain_base(
        &repo,
        &session_dir,
        Some("origin/feature/pr-base"),
    )
    .expect("chain PR worktree must be created from the selected origin ref");

    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&wt)
        .output()
        .unwrap();
    let head_s = String::from_utf8_lossy(&head.stdout).trim().to_string();
    let pr_base = Command::new("git")
        .args(["rev-parse", "origin/feature/pr-base"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let pr_base_s = String::from_utf8_lossy(&pr_base.stdout).trim().to_string();
    assert_eq!(
        head_s, pr_base_s,
        "worktree HEAD must equal the selected remote-tracking ref"
    );
    assert!(
        merge_base_is_ancestor(&repo, &head_s, "origin/master"),
        "selected base should still descend from master"
    );

    let _ = fs::remove_dir_all(&base);
}

/// **changeset_persists_worktree_base_choice** — User-selected base is written to `changeset.yaml`.
#[test]
fn chain_pr_changeset_persists_worktree_base_choice() {
    let base = temp_dir("persist-choice");
    let repo = base.join("repo");
    init_origin_master_and_feature_pr_base(&repo);

    let session_dir = base.join("plan");
    fs::create_dir_all(&session_dir).unwrap();
    let cs = session_with_suggestions("Persist", "feature/persist", "feature-persist");
    write_changeset(&session_dir, &cs).unwrap();
    fs::write(session_dir.join("PRD.md"), "# PRD\n").unwrap();

    setup_worktree_for_session_with_optional_chain_base(
        &repo,
        &session_dir,
        Some("origin/feature/pr-base"),
    )
    .expect("setup with chain base must succeed");

    let yaml = fs::read_to_string(session_dir.join("changeset.yaml")).unwrap();
    assert!(
        yaml.contains("worktree_integration_base_ref:") && yaml.contains("origin/feature/pr-base"),
        "changeset must persist canonical user-selected base ref; got:\n{}",
        yaml
    );

    let cs_after = read_changeset(&session_dir).unwrap();
    assert!(
        cs_after.worktree.is_some() && cs_after.branch.is_some(),
        "worktree setup must populate changeset worktree and branch"
    );

    let _ = fs::remove_dir_all(&base);
}

/// **invalid_base_ref_rejected** — Multi-segment refs must be accepted when safe; empty rejected.
#[test]
fn chain_pr_invalid_base_ref_rejected() {
    assert!(
        validate_chain_pr_integration_base_ref("origin/feature/foo").is_ok(),
        "chain PRs require multi-segment origin refs"
    );
    assert!(
        validate_chain_pr_integration_base_ref("").is_err(),
        "empty ref must be rejected"
    );
}

/// **resume_uses_persisted_base** — Resume must not silently fall back to default main when a chain base was stored.
#[test]
fn chain_pr_resume_uses_persisted_base() {
    let base = temp_dir("resume-base");
    let repo = base.join("repo");
    init_origin_master_and_feature_pr_base(&repo);

    let session_dir = base.join("plan");
    fs::create_dir_all(&session_dir).unwrap();
    let mut cs = session_with_suggestions("Resume", "feature/resume", "feature-resume");
    cs.effective_worktree_integration_base_ref = Some("origin/feature/pr-base".to_string());
    cs.worktree_integration_base_ref = Some("origin/feature/pr-base".to_string());
    write_changeset(&session_dir, &cs).unwrap();
    fs::write(session_dir.join("PRD.md"), "# PRD\n").unwrap();

    let resolved = resolve_persisted_worktree_integration_base_for_session(&session_dir, &repo)
        .expect("resume must resolve persisted integration base");
    let default = resolve_default_integration_base_ref(&repo).unwrap();
    assert_ne!(
        resolved, default,
        "after persisting a chain base, resume must not equal bare default resolution"
    );

    let _ = fs::remove_dir_all(&base);
}
