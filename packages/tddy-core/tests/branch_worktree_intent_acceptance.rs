//! PRD acceptance: git worktree behavior + elicitation contracts for branch/worktree intent.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_core::changeset::{write_changeset, Changeset};
use tddy_core::worktree::{create_worktree, setup_worktree_for_session_with_integration_base};

fn scratch(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "tddy-bw-intent-core-{}-{}",
        label,
        std::process::id()
    ));
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

/// PRD: duplicate worktree path must surface explicit reuse confirmation (not only a bare error).
#[test]
fn worktree_reuse_confirmation_required_when_path_exists() {
    let base = scratch("reuse");
    let repo = base.join("repo");
    fs::create_dir_all(&repo).unwrap();
    init_repo_with_main(&repo);

    let _first =
        create_worktree(&repo, "dup", "feature/dup", Some("origin/main")).expect("first add");
    let err = create_worktree(&repo, "dup", "feature/other", Some("origin/main")).unwrap_err();
    let _ = fs::remove_dir_all(&base);
    assert!(
        err.to_lowercase().contains("reuse") && err.to_lowercase().contains("confirm"),
        "PRD: when a worktree path already exists, the user must surface reuse + confirmation (not only a bare path error); got: {err}"
    );
}

/// PRD intent `new_branch_from_base`: created branch name and start-point must follow persisted
/// `new_branch_name` / base ref (not silently ignore workflow intent).
#[test]
fn create_new_branch_intent_uses_base_ref_and_new_branch_name() {
    let base = scratch("new-branch-intent");
    let repo = base.join("repo");
    fs::create_dir_all(&repo).unwrap();
    init_repo_with_main(&repo);

    let session_dir = base.join("session");
    fs::create_dir_all(&session_dir).unwrap();

    let cs = Changeset {
        name: Some("Acceptance".into()),
        branch_suggestion: Some("feature/ignored-by-intent".into()),
        worktree_suggestion: Some("intent-wt".into()),
        ..Default::default()
    };
    write_changeset(&session_dir, &cs).unwrap();

    let workflow_yaml = r#"
run_optional_step_x: false
demo_options: []
tool_schema_id: urn:tddy:tool/changeset-workflow
branch_worktree_intent: new_branch_from_base
selected_integration_base_ref: origin/main
new_branch_name: feature/custom-from-intent
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
        .expect("setup must succeed once intent-aware worktree creation exists");

    let branch = git(&wt, &["branch", "--show-current"]);
    assert_eq!(
        branch, "feature/custom-from-intent",
        "new_branch_from_base must create/checkout new_branch_name from workflow; got {branch}"
    );
    let tip = git(&repo, &["rev-parse", "origin/main"]);
    let merge_base = git(&wt, &["merge-base", "HEAD", "origin/main"]);
    assert_eq!(
        merge_base, tip,
        "new branch must start from integration base tip"
    );

    let _ = fs::remove_dir_all(&base);
}

/// PRD intent `work_on_selected_branch`: attach a new worktree checked out to the selected branch
/// (existing branch), not a freshly created feature branch name.
#[test]
fn work_on_selected_branch_intent_checks_out_existing_branch_in_new_worktree() {
    let base = scratch("work-on-selected");
    let repo = base.join("repo");
    fs::create_dir_all(&repo).unwrap();
    init_repo_with_main(&repo);

    let session_dir = base.join("session");
    fs::create_dir_all(&session_dir).unwrap();

    let cs = Changeset {
        name: Some("WorkOnSelected".into()),
        branch_suggestion: Some("main".into()),
        worktree_suggestion: Some("main-wt".into()),
        ..Default::default()
    };
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

    let wt = setup_worktree_for_session_with_integration_base(&repo, &session_dir, "origin/main")
        .expect("setup must attach worktree to existing branch per PRD");

    assert_eq!(git(&wt, &["branch", "--show-current"]), "main");
    let wt_head = git(&wt, &["rev-parse", "HEAD"]);
    let main_head = git(&repo, &["rev-parse", "main"]);
    assert_eq!(
        wt_head, main_head,
        "worktree HEAD must match selected branch tip"
    );

    let _ = fs::remove_dir_all(&base);
}

/// PRD: RPC/proto must expose intent so web and daemon clients stay consistent with changeset.yaml.
#[test]
fn elicitation_payload_includes_intent_for_clients() {
    const REMOTE_PROTO: &str = include_str!("../../tddy-service/proto/tddy/v1/remote.proto");
    assert!(
        REMOTE_PROTO.contains("branch_worktree_intent")
            || REMOTE_PROTO.contains("BranchWorktreeIntent"),
        "WorktreeElicitation (or equivalent) must include branch/worktree intent fields for clients"
    );
}
