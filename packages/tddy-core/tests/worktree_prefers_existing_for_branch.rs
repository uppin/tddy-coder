//! When `work_on_selected_branch` would create `.worktrees/<new>/`, prefer an existing linked
//! worktree whose HEAD matches the selected ref (same commit as `origin/feature/x`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_core::changeset::{write_changeset, Changeset};
use tddy_core::worktree::{
    setup_worktree_for_session_with_integration_base,
    setup_worktree_for_session_with_optional_chain_base,
};

fn scratch(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("tddy-wt-pref-{}-{}", label, std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn init_repo_with_main(repo: &Path) {
    Command::new("git")
        .args(["init"])
        .current_dir(repo)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "t@t.com"])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "T"])
        .current_dir(repo)
        .output()
        .unwrap();
    fs::write(repo.join("f"), "x").unwrap();
    Command::new("git")
        .args(["add", "f"])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "c"])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["remote", "add", "origin", repo.to_str().unwrap()])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(repo)
        .output()
        .unwrap();
}

#[test]
fn work_on_selected_reuses_existing_worktree_at_branch_tip_instead_of_second_add() {
    let base = scratch("prefer");
    let repo = base.join("repo");
    fs::create_dir_all(&repo).unwrap();
    init_repo_with_main(&repo);

    Command::new("git")
        .args(["checkout", "-b", "feature/oauth-test", "origin/main"])
        .current_dir(&repo)
        .output()
        .expect("branch");
    fs::write(repo.join("f2"), "y").unwrap();
    Command::new("git")
        .args(["add", "f2"])
        .current_dir(&repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "feat"])
        .current_dir(&repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["push", "-u", "origin", "feature/oauth-test"])
        .current_dir(&repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let pre_existing = repo.join(".worktrees").join("already-there");
    Command::new("git")
        .args([
            "worktree",
            "add",
            pre_existing.to_str().unwrap(),
            "feature/oauth-test",
        ])
        .current_dir(&repo)
        .output()
        .expect("worktree add");

    let session_dir = base.join("session");
    fs::create_dir_all(&session_dir).unwrap();

    let mut cs = Changeset::default();
    cs.name = Some("MergePR".into());
    cs.worktree_suggestion = Some("merge-pr-would-use-this-basename".into());
    write_changeset(&session_dir, &cs).unwrap();

    let workflow_yaml = r#"
run_optional_step_x: false
demo_options: []
tool_schema_id: urn:tddy:tool/changeset-workflow
branch_worktree_intent: work_on_selected_branch
selected_branch_to_work_on: origin/feature/oauth-test
"#;
    let path = session_dir.join("changeset.yaml");
    let mut raw = fs::read_to_string(&path).unwrap();
    raw.push_str("workflow:\n");
    for line in workflow_yaml.lines() {
        raw.push_str("  ");
        raw.push_str(line);
        raw.push('\n');
    }
    fs::write(&path, raw).unwrap();

    let wt = setup_worktree_for_session_with_integration_base(&repo, &session_dir, "origin/main")
        .expect("setup must reuse pre-existing worktree at same tip as selected ref");

    assert_eq!(
        wt.canonicalize().unwrap_or_else(|_| wt.clone()),
        pre_existing
            .canonicalize()
            .unwrap_or_else(|_| pre_existing.clone()),
        "must not create a second worktree under a new basename when one already holds the branch tip"
    );

    let list = Command::new("git")
        .args(["worktree", "list"])
        .current_dir(&repo)
        .output()
        .expect("worktree list");
    let list_s = String::from_utf8_lossy(&list.stdout);
    assert_eq!(
        list_s.lines().count(),
        2,
        "expected exactly main repo + one linked worktree; got:\n{list_s}"
    );

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn optional_chain_base_path_reuses_existing_worktree_for_work_on_selected() {
    let base = scratch("optional-chain");
    let repo = base.join("repo");
    fs::create_dir_all(&repo).unwrap();
    init_repo_with_main(&repo);

    Command::new("git")
        .args(["checkout", "-b", "feature/oauth-chain", "origin/main"])
        .current_dir(&repo)
        .output()
        .expect("branch");
    fs::write(repo.join("fc"), "z").unwrap();
    Command::new("git")
        .args(["add", "fc"])
        .current_dir(&repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "feat"])
        .current_dir(&repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["push", "-u", "origin", "feature/oauth-chain"])
        .current_dir(&repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let pre_existing = repo.join(".worktrees").join("chain-existing");
    Command::new("git")
        .args([
            "worktree",
            "add",
            pre_existing.to_str().unwrap(),
            "feature/oauth-chain",
        ])
        .current_dir(&repo)
        .output()
        .expect("worktree add");

    let session_dir = base.join("session");
    fs::create_dir_all(&session_dir).unwrap();

    let mut cs = Changeset::default();
    cs.name = Some("MergePR".into());
    cs.worktree_suggestion = Some("merge-pr-would-use-this-basename".into());
    write_changeset(&session_dir, &cs).unwrap();

    let workflow_yaml = r#"
run_optional_step_x: false
demo_options: []
tool_schema_id: urn:tddy:tool/changeset-workflow
branch_worktree_intent: work_on_selected_branch
selected_branch_to_work_on: origin/feature/oauth-chain
"#;
    let path = session_dir.join("changeset.yaml");
    let mut raw = fs::read_to_string(&path).unwrap();
    raw.push_str("workflow:\n");
    for line in workflow_yaml.lines() {
        raw.push_str("  ");
        raw.push_str(line);
        raw.push('\n');
    }
    fs::write(&path, raw).unwrap();

    let wt = setup_worktree_for_session_with_optional_chain_base(&repo, &session_dir, None)
        .expect("optional-chain setup must reuse pre-existing worktree");

    assert_eq!(
        wt.canonicalize().unwrap_or_else(|_| wt.clone()),
        pre_existing
            .canonicalize()
            .unwrap_or_else(|_| pre_existing.clone()),
        "merge-pr-style entrypoint must not add a second worktree when one already matches the branch tip"
    );

    let list = Command::new("git")
        .args(["worktree", "list"])
        .current_dir(&repo)
        .output()
        .expect("worktree list");
    let list_s = String::from_utf8_lossy(&list.stdout);
    assert_eq!(
        list_s.lines().count(),
        2,
        "expected exactly main repo + one linked worktree; got:\n{list_s}"
    );

    let _ = fs::remove_dir_all(&base);
}
