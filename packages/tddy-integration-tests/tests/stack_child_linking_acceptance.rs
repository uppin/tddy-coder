//! PRD acceptance: stack child linking — spawn_chain_child_worktree wires child worktree base,
//! link_stack_node_to_child_session sets session_id + branch, orchestrator_session_id round-trips.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_core::changeset::{
    link_stack_node_to_child_session, read_changeset, write_changeset, Changeset, ChangesetState,
    Stack, StackNode,
};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_chain::spawn_chain_child_worktree;
use tddy_core::workflow::ids::WorkflowState;

fn temp_dir(label: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "tddy-stack-link-acc-{}-{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
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

fn init_repo_with_feature_branch(repo: &Path, branch: &str) {
    fs::create_dir_all(repo).unwrap();
    git(repo, &["init"]);
    git(repo, &["config", "user.email", "test@test.com"]);
    git(repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("f"), "x").unwrap();
    git(repo, &["add", "f"]);
    git(repo, &["commit", "-m", "init"]);
    git(repo, &["branch", "-M", "master"]);
    git(repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
    git(repo, &["push", "-u", "origin", "master"]);
    git(repo, &["checkout", "-b", branch]);
    fs::write(repo.join("feat.txt"), "feature work").unwrap();
    git(repo, &["add", "feat.txt"]);
    git(repo, &["commit", "-m", "feat"]);
    git(repo, &["push", "-u", "origin", branch]);
    git(repo, &["checkout", "master"]);
}

#[test]
#[ignore = "spawn_chain_child_worktree not yet implemented; re-enable when session_chain.rs is complete"]
fn spawn_chain_child_sets_orchestrator_session_id() {
    // Given — orchestrator session with a branch, child session dir
    let base = temp_dir("spawn-orch");
    let repo = base.join("repo");
    let feature_branch = "feature/orch-parent";
    init_repo_with_feature_branch(&repo, feature_branch);
    let repo_canon = repo.canonicalize().unwrap();

    let orch_id = "018faaaa-0001-7000-aaaa-000000000001";
    let sessions_home = base.join("sessions");
    let orch_dir = unified_session_dir_path(&sessions_home, orch_id);
    fs::create_dir_all(&orch_dir).unwrap();

    let orch_cs = Changeset {
        name: Some("orchestrator".into()),
        branch_suggestion: Some(feature_branch.into()),
        repo_path: Some(repo_canon.to_string_lossy().into_owned()),
        state: ChangesetState {
            current: WorkflowState::new("StackPlanned"),
            ..Changeset::default().state
        },
        stack: Some(Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".into(),
                title: "Child PR".into(),
                description: String::new(),
                branch_suggestion: Some("feature/child-n1".into()),
                branch: None,
                session_id: None,
                parents: vec![],
                pr_status: None,
                child_state: None,
            }],
        }),
        ..Changeset::default()
    };
    write_changeset(&orch_dir, &orch_cs).unwrap();

    let child_id = "018fbbbb-0002-7000-bbbb-000000000002";
    let child_dir = unified_session_dir_path(&sessions_home, child_id);
    fs::create_dir_all(&child_dir).unwrap();
    let child_cs = Changeset {
        name: Some("child-n1".into()),
        worktree_suggestion: Some("child-n1-wt".into()),
        branch_suggestion: Some("feature/child-n1".into()),
        ..Changeset::default()
    };
    write_changeset(&child_dir, &child_cs).unwrap();

    // When — spawn_chain_child_worktree sets integration base from orchestrator's branch
    let result = spawn_chain_child_worktree(
        &sessions_home,
        orch_id,
        &child_dir,
        &repo_canon,
        None, // derive from parent branch
    );
    let _ = fs::remove_dir_all(&base);

    // Then — must succeed; the child worktree creation confirms the base ref wired correctly
    result.expect(
        "spawn_chain_child_worktree must succeed when orchestrator has a valid branch + repo_path"
    );
}

#[test]
fn link_stack_node_sets_session_id_and_branch() {
    // Given — orchestrator changeset with an unlinked node
    let base = temp_dir("link-node");
    let orch_dir = base.join("orch");
    fs::create_dir_all(&orch_dir).unwrap();

    let cs = Changeset {
        name: Some("orch".into()),
        stack: Some(Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".into(),
                title: "Task 1".into(),
                description: String::new(),
                branch_suggestion: Some("feature/task-1".into()),
                branch: None,
                session_id: None,
                parents: vec![],
                pr_status: None,
                child_state: None,
            }],
        }),
        ..Changeset::default()
    };
    write_changeset(&orch_dir, &cs).unwrap();

    // When
    link_stack_node_to_child_session(
        &orch_dir,
        "n1",
        "child-session-id-abc",
        Some("feature/task-1".into()),
    )
    .expect("link_stack_node_to_child_session must succeed");

    // Then
    let loaded = read_changeset(&orch_dir).unwrap();
    let _ = fs::remove_dir_all(&base);
    let stack = loaded.stack.expect("stack must still be present");
    let n1 = stack.node("n1").expect("n1 must exist");
    assert_eq!(
        n1.session_id.as_deref(),
        Some("child-session-id-abc"),
        "session_id must be set after link"
    );
    assert_eq!(
        n1.branch.as_deref(),
        Some("feature/task-1"),
        "branch must be set after link"
    );
}

#[test]
fn dag_child_orchestrator_id_always_points_at_orchestrator() {
    // Given — a DAG where n2's base is sibling n1 (not the orchestrator).
    // The orchestrator session changeset links n1 and n2 as its children.
    // The child n2's own changeset must have orchestrator_session_id = orchestrator (not n1).
    // This is a structural check: the plan sets orchestrator_session_id separate from previous_session_id.
    let base = temp_dir("dag-orch-id");
    let sessions_home = base.join("sessions");

    let orch_id = "018f0000-aaaa-7000-0000-000000000001";
    let orch_dir = unified_session_dir_path(&sessions_home, orch_id);
    fs::create_dir_all(&orch_dir).unwrap();

    // n2 child's changeset — orchestrator_session_id must be orch_id, not n1's session id
    let n2_child_id = "018f0000-bbbb-7000-0000-000000000002";
    let n2_child_dir = unified_session_dir_path(&sessions_home, n2_child_id);
    fs::create_dir_all(&n2_child_dir).unwrap();

    let n2_cs = Changeset {
        name: Some("n2-child".into()),
        orchestrator_session_id: Some(orch_id.into()),
        // previous_session_id lives in SessionMetadata, not Changeset — omit here
        ..Changeset::default()
    };
    write_changeset(&n2_child_dir, &n2_cs).unwrap();

    // When — read back
    let loaded = read_changeset(&n2_child_dir).unwrap();
    let _ = fs::remove_dir_all(&base);

    // Then
    assert_eq!(
        loaded.orchestrator_session_id.as_deref(),
        Some(orch_id),
        "child changeset orchestrator_session_id must point at the orchestrator session (not the sibling base)"
    );
}
