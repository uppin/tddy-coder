//! Granular Phase 2 session-chaining tests (GREEN).

use std::path::Path;
use std::process::Command;

use tddy_core::changeset::{write_changeset, Changeset};
use tddy_core::session_lifecycle::unified_session_dir_path;

fn run_git(repo: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git");
    assert!(
        o.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&o.stderr)
    );
}

/// Minimal repo with `feature/x` pushed to `origin` (same layout as `tddy_core::session_chain` tests).
fn scratch_repo_with_feature_x(base: &Path) -> std::path::PathBuf {
    let repo = base.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init"]);
    run_git(&repo, &["config", "user.email", "t@t.com"]);
    run_git(&repo, &["config", "user.name", "T"]);
    std::fs::write(repo.join("README"), "x").unwrap();
    run_git(&repo, &["add", "README"]);
    run_git(&repo, &["commit", "-m", "init"]);
    run_git(&repo, &["branch", "-M", "master"]);
    run_git(&repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
    run_git(&repo, &["push", "-u", "origin", "master"]);
    run_git(&repo, &["checkout", "-b", "feature/x"]);
    std::fs::write(repo.join("f"), "y").unwrap();
    run_git(&repo, &["add", "f"]);
    run_git(&repo, &["commit", "-m", "feat"]);
    run_git(&repo, &["push", "-u", "origin", "feature/x"]);
    run_git(&repo, &["checkout", "master"]);
    repo
}

#[test]
fn phase2_live_tcp_dispatch_ready_is_false_until_green() {
    tddy_daemon::telegram_bot::chain_phase2_tcp_dispatch_marker_probe();
    assert!(
        tddy_daemon::telegram_bot::session_chaining_phase2_live_tcp_dispatch_ready(),
        "GREEN: live tcp: dispatch enabled for chain parent callbacks"
    );
}

#[test]
fn phase2_chain_base_merge_ready_is_false_until_green() {
    assert!(
        tddy_daemon::telegram_session_control::session_chaining_phase2_chain_base_merge_ready(),
        "GREEN: chain base merge wired for Telegram spawn"
    );
}

#[test]
fn merge_chain_integration_base_returns_ok_for_placeholder_paths_in_green() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = scratch_repo_with_feature_x(tmp.path());
    let repo_canon = repo.canonicalize().expect("canon");

    let session_base = tmp.path();
    let parent_id = "parent-session-merge-test";
    let parent_dir = unified_session_dir_path(session_base, parent_id);
    std::fs::create_dir_all(&parent_dir).unwrap();
    let parent_cs = Changeset {
        branch: Some("feature/x".into()),
        repo_path: Some(repo_canon.to_string_lossy().into()),
        ..Changeset::default()
    };
    write_changeset(&parent_dir, &parent_cs).expect("parent changeset");

    let child_dir = unified_session_dir_path(session_base, "child-merge-test");
    std::fs::create_dir_all(&child_dir).unwrap();
    let child_cs = Changeset {
        name: Some("child".into()),
        branch_suggestion: Some("feature/child".into()),
        ..Changeset::default()
    };
    write_changeset(&child_dir, &child_cs).expect("child changeset");

    tddy_daemon::telegram_session_control::merge_chain_integration_base_with_explicit_operator_overrides(
        session_base,
        parent_id,
        &child_dir,
        &repo_canon,
        None,
    )
    .expect("GREEN: merge resolves parent chain ref and integrates when paths exist");
}
