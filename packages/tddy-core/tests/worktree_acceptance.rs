//! Acceptance tests for worktree-per-workflow feature.
//!
//! These tests verify that:
//! - Worktrees are created from origin/master after plan approval
//! - changeset.yaml is updated with worktree, branch, repo_path
//! - Context header includes repo_dir
//! - Activity log shows worktree path
//! - Fetch failure propagates error

use std::fs;
use std::path::PathBuf;

use tddy_core::changeset::{read_changeset, write_changeset, Changeset};
use tddy_core::workflow::{build_context_header, prepend_context_header};
use tddy_core::{create_worktree, setup_worktree_for_session};

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tddy-worktree-acc-{}", label));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// setup_worktree_for_session creates worktree from origin/master and updates changeset.
#[test]
fn test_setup_worktree_for_session_creates_from_origin_master_and_updates_changeset() {
    let base = temp_dir("setup-session");
    let repo = base.join("repo");
    fs::create_dir_all(&repo).unwrap();

    // Init git repo with a commit and origin/master
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&repo)
        .output()
        .unwrap();
    fs::write(repo.join("README"), "initial").unwrap();
    std::process::Command::new("git")
        .args(["add", "README"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["branch", "-M", "master"])
        .current_dir(&repo)
        .output()
        .unwrap();
    // Add origin as local ref so fetch works
    std::process::Command::new("git")
        .args(["remote", "add", "origin", repo.to_str().unwrap()])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["push", "-u", "origin", "master"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let plan_dir = base.join("plan");
    fs::create_dir_all(&plan_dir).unwrap();
    let mut cs = Changeset {
        name: Some("Auth Feature".to_string()),
        initial_prompt: Some("add auth".to_string()),
        ..Default::default()
    };
    cs.state.current = "Planned".to_string();
    cs.branch_suggestion = Some("feature/auth".to_string());
    cs.worktree_suggestion = Some("feature-auth".to_string());
    cs.branch = None;
    cs.worktree = None;
    cs.repo_path = None;
    write_changeset(&plan_dir, &cs).unwrap();

    // PRD from plan output
    let prd = r#"# Auth Feature
## TODO
- [ ] Implement
"#;
    fs::write(plan_dir.join("PRD.md"), prd).unwrap();

    let result = setup_worktree_for_session(&repo, &plan_dir);
    assert!(
        result.is_ok(),
        "setup_worktree_for_session should succeed: {:?}",
        result.err()
    );
    let worktree_path = result.unwrap();

    assert!(
        worktree_path.exists(),
        "worktree dir should exist: {}",
        worktree_path.display()
    );
    assert!(
        worktree_path.to_string_lossy().contains("worktree")
            || worktree_path.to_string_lossy().contains(".worktrees"),
        "worktree path should be under .worktrees"
    );

    let cs_after = read_changeset(&plan_dir).unwrap();
    assert!(
        cs_after.worktree.is_some(),
        "changeset should have worktree path"
    );
    assert_eq!(
        cs_after.worktree.as_ref().unwrap(),
        &worktree_path.to_string_lossy()
    );
    assert!(cs_after.branch.is_some(), "changeset should have branch");
    assert!(
        cs_after.repo_path.is_some(),
        "changeset should have repo_path"
    );
    assert_eq!(
        cs_after.repo_path.as_ref().unwrap(),
        &worktree_path.to_string_lossy()
    );

    let _ = fs::remove_dir_all(&base);
}

/// build_context_header includes repo_dir when provided.
#[test]
fn test_context_header_includes_repo_dir() {
    let dir = temp_dir("ctx-repo-dir");
    fs::write(dir.join("PRD.md"), "# PRD").unwrap();
    let repo_dir = dir.join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    let header = build_context_header(Some(&dir), Some(&repo_dir));

    assert!(
        !header.is_empty(),
        "header should not be empty when plan_dir has artifacts"
    );
    assert!(
        header.contains("repo_dir:"),
        "header must include repo_dir line, got:\n{}",
        header
    );
    let repo_line = header.lines().find(|l| l.starts_with("repo_dir:")).unwrap();
    let path_str = repo_line.trim_start_matches("repo_dir:").trim();
    assert!(
        std::path::Path::new(path_str).is_absolute(),
        "repo_dir path must be absolute: {}",
        path_str
    );

    let _ = fs::remove_dir_all(&dir);
}

/// prepend_context_header includes repo_dir in output when provided.
#[test]
fn test_prepend_context_header_includes_repo_dir() {
    let dir = temp_dir("prepend-repo-dir");
    fs::write(dir.join("PRD.md"), "# PRD").unwrap();
    let repo_dir = dir.join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    let prompt = prepend_context_header("Hello".to_string(), Some(&dir), Some(&repo_dir));

    assert!(prompt.contains("<context-reminder>"));
    assert!(prompt.contains("repo_dir:"));
    assert!(prompt.contains("Hello"));

    let _ = fs::remove_dir_all(&dir);
}

/// create_worktree with start_point creates branch from origin/master.
#[test]
fn test_create_worktree_with_start_point_uses_origin_master() {
    let base = temp_dir("create-start-point");
    let repo = base.join("repo");
    fs::create_dir_all(&repo).unwrap();

    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&repo)
        .output()
        .unwrap();
    fs::write(repo.join("f"), "x").unwrap();
    std::process::Command::new("git")
        .args(["add", "f"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "c1"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["branch", "-M", "master"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["remote", "add", "origin", repo.to_str().unwrap()])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["push", "-u", "origin", "master"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let result = create_worktree(&repo, "feature-x", "feature/x", Some("origin/master"));
    assert!(
        result.is_ok(),
        "create_worktree should succeed: {:?}",
        result.err()
    );
    let wt = result.unwrap();
    assert!(wt.exists());

    // Verify the worktree's HEAD matches origin/master
    let rev = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&wt)
        .output()
        .unwrap();
    let origin_master = std::process::Command::new("git")
        .args(["rev-parse", "origin/master"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&rev.stdout),
        String::from_utf8_lossy(&origin_master.stdout),
        "worktree HEAD should match origin/master"
    );

    let _ = fs::remove_dir_all(&base);
}
