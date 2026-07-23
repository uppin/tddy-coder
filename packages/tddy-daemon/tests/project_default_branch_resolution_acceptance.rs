//! Acceptance: the project default branch (`main_branch_ref`) is the single source of truth for a
//! project's integration base ref, and the live origin/master→origin/main→origin/HEAD probe is
//! **legacy-only** — it resolves the default for projects that have no stored branch and loses
//! effect once one is set.
//!
//! These tests reference only existing symbols, so they compile against today's code and fail at
//! runtime: today a legacy project resolves to the hardcoded `origin/master` regardless of what the
//! repository actually has, and `add_project` rejects multi-segment refs.
//!
//! PRD: docs/ft/coder/git-integration-base-ref.md (Unified resolution, Validation).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use tddy_daemon::project_storage::{
    add_project, effective_integration_base_ref_for_project, ProjectData,
};

fn require_git() {
    let ok = Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(
        ok,
        "git must be available for default-branch resolution tests"
    );
}

fn run_git(cwd: &Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} in {cwd:?}: {e}"));
    assert!(st.success(), "git {args:?} failed in {cwd:?}");
}

/// A source repo carrying exactly `branches` (the first is the initial commit's branch). Returned
/// as a local `git_url` a clone can point `origin` at.
fn a_source_repo_with_branches(dir: &Path, branches: &[&str]) -> String {
    require_git();
    std::fs::create_dir_all(dir).unwrap();
    run_git(dir, &["init", "-b", branches[0]]);
    run_git(dir, &["config", "user.email", "t@e.st"]);
    run_git(dir, &["config", "user.name", "t"]);
    std::fs::write(dir.join("README.md"), "x\n").unwrap();
    run_git(dir, &["add", "README.md"]);
    run_git(dir, &["commit", "-m", "init"]);
    for b in &branches[1..] {
        run_git(dir, &["branch", b]);
    }
    dir.to_str().unwrap().to_string()
}

/// Clone `source` into `dest` so `dest` has remote-tracking `origin/<branch>` refs, and return the
/// clone path as a string.
fn a_clone(source: &str, dest: &Path) -> String {
    run_git(
        dest.parent().unwrap(),
        &["clone", source, dest.to_str().unwrap()],
    );
    dest.to_str().unwrap().to_string()
}

fn a_projects_dir() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("projects");
    std::fs::create_dir_all(&dir).unwrap();
    (temp, dir)
}

#[test]
fn legacy_project_resolves_its_default_live_from_the_repository_not_a_hardcoded_master() {
    // Given a repo whose only mainline branch is `main` (no `master`), cloned locally, and a legacy
    // project row with no stored default branch.
    let src_dir = tempfile::tempdir().unwrap();
    let source = a_source_repo_with_branches(&src_dir.path().join("src"), &["main"]);
    let clone_dir = tempfile::tempdir().unwrap();
    let repo = a_clone(&source, &clone_dir.path().join("clone"));
    let (_keep, projects_dir) = a_projects_dir();
    add_project(
        &projects_dir,
        ProjectData {
            project_id: "legacy-1".to_string(),
            name: "alpha".to_string(),
            git_url: source.clone(),
            main_repo_path: repo,
            main_branch_ref: None,
            host_repo_paths: HashMap::new(),
        },
    )
    .expect("register legacy project");

    // When
    let resolved =
        effective_integration_base_ref_for_project(&projects_dir, "legacy-1").expect("resolve");

    // Then — the live probe picks the branch the repo actually has, not a hardcoded origin/master.
    assert_eq!(resolved, "origin/main");
}

#[test]
fn a_stored_default_branch_wins_over_the_legacy_probe() {
    // Given a repo that DOES have origin/master (the probe would pick it), but the project stores a
    // different — and slash-containing — branch as its default.
    let src_dir = tempfile::tempdir().unwrap();
    let source = a_source_repo_with_branches(
        &src_dir.path().join("src"),
        &["master", "main", "release/2025"],
    );
    let clone_dir = tempfile::tempdir().unwrap();
    let repo = a_clone(&source, &clone_dir.path().join("clone"));
    let (_keep, projects_dir) = a_projects_dir();
    add_project(
        &projects_dir,
        ProjectData {
            project_id: "set-1".to_string(),
            name: "alpha".to_string(),
            git_url: source.clone(),
            main_repo_path: repo,
            main_branch_ref: Some("origin/release/2025".to_string()),
            host_repo_paths: HashMap::new(),
        },
    )
    .expect("register project with stored default");

    // When
    let resolved =
        effective_integration_base_ref_for_project(&projects_dir, "set-1").expect("resolve");

    // Then — the stored ref is authoritative; the probe (which would pick origin/master) never runs.
    assert_eq!(resolved, "origin/release/2025");
}

#[test]
fn any_remote_branch_including_a_slashed_name_is_a_valid_stored_default() {
    // Given a project whose stored default branch is a multi-segment remote branch
    let (_keep, projects_dir) = a_projects_dir();

    // When
    let result = add_project(
        &projects_dir,
        ProjectData {
            project_id: "slashed-1".to_string(),
            name: "alpha".to_string(),
            git_url: "https://example.com/a.git".to_string(),
            main_repo_path: "/tmp/a".to_string(),
            main_branch_ref: Some("origin/release/2025".to_string()),
            host_repo_paths: HashMap::new(),
        },
    );

    // Then — a slash-containing remote branch is accepted and persisted as the default.
    result.expect("multi-segment origin/<path> must be a legal project default branch");
}

#[test]
fn a_default_branch_with_shell_metacharacters_is_still_rejected() {
    // Given a project whose stored default branch carries a shell-injection payload
    let (_keep, projects_dir) = a_projects_dir();

    // When
    let result = add_project(
        &projects_dir,
        ProjectData {
            project_id: "bad-1".to_string(),
            name: "alpha".to_string(),
            git_url: "https://example.com/a.git".to_string(),
            main_repo_path: "/tmp/a".to_string(),
            main_branch_ref: Some("origin/main;rm -rf /".to_string()),
            host_repo_paths: HashMap::new(),
        },
    );

    // Then
    assert!(
        result.is_err(),
        "unsafe default branch refs must be rejected before any YAML write"
    );
}
