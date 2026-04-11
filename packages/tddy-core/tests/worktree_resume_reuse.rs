//! Resume: when `changeset.worktree` is missing but a git-linked worktree already exists at the
//! derived path, session setup must detect and reuse it instead of failing with "path already exists".

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_core::changeset::{read_changeset, write_changeset, Changeset};
use tddy_core::worktree::setup_worktree_for_session_with_integration_base;

fn scratch(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("tddy-wt-resume-{}-{}", label, std::process::id()));
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
fn setup_worktree_reuses_existing_linked_worktree_when_changeset_omits_worktree_path() {
    let base = scratch("reuse");
    let repo = base.join("repo");
    fs::create_dir_all(&repo).unwrap();
    init_repo_with_main(&repo);

    let session_dir = base.join("session");
    fs::create_dir_all(&session_dir).unwrap();

    let mut cs = Changeset::default();
    cs.name = Some("ResumeReuse".into());
    cs.branch_suggestion = Some("main".into());
    cs.worktree_suggestion = Some("resume-wt".into());
    write_changeset(&session_dir, &cs).unwrap();

    let workflow_yaml = r#"
run_optional_step_x: false
demo_options: []
tool_schema_id: urn:tddy:tool/changeset-workflow
branch_worktree_intent: work_on_selected_branch
selected_branch_to_work_on: main
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

    let wt_first =
        setup_worktree_for_session_with_integration_base(&repo, &session_dir, "origin/main")
            .expect("first worktree setup");

    let mut cs = read_changeset(&session_dir).unwrap();
    cs.worktree = None;
    cs.repo_path = None;
    write_changeset(&session_dir, &cs).unwrap();

    let wt_second =
        setup_worktree_for_session_with_integration_base(&repo, &session_dir, "origin/main")
            .expect(
                "must reuse existing git worktree when the directory is already linked; got error",
            );

    assert_eq!(
        wt_first.canonicalize().unwrap_or_else(|_| wt_first.clone()),
        wt_second
            .canonicalize()
            .unwrap_or_else(|_| wt_second.clone()),
        "resume must return the same worktree path as the first setup"
    );

    let _ = fs::remove_dir_all(&base);
}
