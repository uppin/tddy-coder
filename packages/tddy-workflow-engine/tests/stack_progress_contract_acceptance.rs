//! PRD acceptance: stack progress contract — sync_stack_node_from_child mirrors child session
//! state + PR status into orchestrator StackNode; update_stack_atomic applies mutations atomically.

use std::fs;
use std::path::PathBuf;

use tddy_changeset::changeset::{
    sync_stack_node_from_child, update_stack_atomic, write_changeset, Changeset, ChangesetState,
    ChangesetWorkflow, GithubPrStatus, Stack, StackNode,
};
use tddy_changeset::session_lifecycle::unified_session_dir_path;
use tddy_workflow_engine::workflow::ids::WorkflowState;

fn scratch(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "tddy-stack-progress-acc-{}-{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn sync_stack_node_reflects_child_state_and_pr_status() {
    // Given — orchestrator session with n1 linked to a child session
    let base = scratch("sync-child");
    let sessions_home = base.join("sessions");

    let orch_id = "018f1111-aaaa-7000-1111-000000000001";
    let orch_dir = unified_session_dir_path(&sessions_home, orch_id);
    fs::create_dir_all(&orch_dir).unwrap();

    let child_id = "018f2222-bbbb-7000-2222-000000000002";
    let child_dir = unified_session_dir_path(&sessions_home, child_id);
    fs::create_dir_all(&child_dir).unwrap();

    // Child session has state=Done and a PR open status
    let child_cs = Changeset {
        name: Some("child-n1".into()),
        state: ChangesetState {
            current: WorkflowState::new("Done"),
            ..Changeset::default().state
        },
        workflow: Some(ChangesetWorkflow {
            github_pr_status: Some(GithubPrStatus {
                phase: "open".into(),
                url: Some("https://github.com/org/repo/pull/42".into()),
                error: None,
            }),
            ..ChangesetWorkflow::default()
        }),
        ..Changeset::default()
    };
    write_changeset(&child_dir, &child_cs).unwrap();

    // Orchestrator: n1 linked to child_id, child_state not yet mirrored
    let orch_cs = Changeset {
        name: Some("orchestrator".into()),
        stack: Some(Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".into(),
                title: "Feature PR".into(),
                description: String::new(),
                branch_suggestion: Some("feature/pr-1".into()),
                branch: Some("feature/pr-1".into()),
                session_id: Some(child_id.into()),
                parents: vec![],
                pr_status: None,   // not yet synced
                child_state: None, // not yet synced
                internal_status: None,
                display_order: None,
            }],
        }),
        ..Changeset::default()
    };
    tddy_changeset::stack_fixtures::write_changeset_with_stack(&orch_dir, orch_cs.clone());

    // When
    sync_stack_node_from_child(&orch_dir, &sessions_home, "n1")
        .expect("sync_stack_node_from_child must succeed");

    // Then
    let stack = tddy_changeset::stack_fixtures::stack_on_disk(&orch_dir);
    let _ = fs::remove_dir_all(&base);
    let n1 = stack.node("n1").expect("n1 must exist");

    let child_state = n1
        .child_state
        .as_ref()
        .expect("child_state must be mirrored from child changeset state.current");
    assert_eq!(
        child_state.as_str(),
        "Done",
        "child_state must reflect child session state.current (Done)"
    );

    let pr_status = n1
        .pr_status
        .as_ref()
        .expect("pr_status must be mirrored from child changeset workflow.github_pr_status");
    assert_eq!(
        pr_status.phase, "open",
        "pr_status.phase must reflect child github_pr_status.phase"
    );
    assert_eq!(
        pr_status.url.as_deref(),
        Some("https://github.com/org/repo/pull/42"),
        "pr_status.url must be preserved"
    );
}

#[test]
fn update_stack_atomic_applies_mutation_and_writes_back() {
    // Given
    let base = scratch("update-atomic");
    let orch_dir = base.join("sessions").join("orch");
    fs::create_dir_all(&orch_dir).unwrap();

    let cs = Changeset {
        name: Some("orch".into()),
        stack: Some(Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".into(),
                title: "PR 1".into(),
                description: String::new(),
                branch_suggestion: None,
                branch: None,
                session_id: None,
                parents: vec![],
                pr_status: None,
                child_state: None,
                internal_status: None,
                display_order: None,
            }],
        }),
        ..Changeset::default()
    };
    tddy_changeset::stack_fixtures::write_changeset_with_stack(&orch_dir, cs.clone());

    // When — mutate the stack atomically
    update_stack_atomic(&orch_dir, |stack| {
        stack.version = 2;
        if let Some(n) = stack.nodes.iter_mut().find(|n| n.node_id == "n1") {
            n.branch = Some("feature/mutated".into());
        }
    })
    .expect("update_stack_atomic must succeed");

    // Then
    let stack = tddy_changeset::stack_fixtures::stack_on_disk(&orch_dir);
    let _ = fs::remove_dir_all(&base);
    assert_eq!(stack.version, 2, "stack.version must be updated atomically");
    let n1 = stack.node("n1").expect("n1");
    assert_eq!(
        n1.branch.as_deref(),
        Some("feature/mutated"),
        "node branch must reflect mutation"
    );
}
