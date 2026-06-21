//! PRD acceptance: orchestrate-pr-stack assess loop — decide_next_action picks Merge for bottom
//! node when ready; returns Wait in operator-gated mode.

use tddy_core::workflow::ids::WorkflowState;
use tddy_workflow_recipes::orchestrate_pr_stack::{
    decide_next_action, ChildPhase, NodeView, OrchestratorAction, PrLiveStatus,
};

#[test]
fn autonomous_merge_on_linear_two_pr_stack_reaches_done() {
    // Given — 2-node linear stack: n1 (bottom, open PR #1, base=master), n2 (top, PR #2, base=n1-branch)
    // autonomous_merge is represented as context — here we call decide_next_action directly with views.
    // n1: all deps merged (none), PR open, base is master → eligible to Merge.
    let views = vec![
        NodeView {
            node_id: "n1".into(),
            branch: "feature/bottom".into(),
            parent_dep_ids: vec![],
            child_session_id: Some("sid-n1".into()),
            child_state: Some(WorkflowState::new("Done")),
            child_phase: ChildPhase::PrOpen,
            pr: PrLiveStatus::Open {
                number: 1,
                base: "master".into(),
            },
        },
        NodeView {
            node_id: "n2".into(),
            branch: "feature/top".into(),
            parent_dep_ids: vec!["n1".into()],
            child_session_id: Some("sid-n2".into()),
            child_state: Some(WorkflowState::new("Done")),
            child_phase: ChildPhase::PrOpen,
            pr: PrLiveStatus::Open {
                number: 2,
                base: "feature/bottom".into(),
            },
        },
    ];

    // When — n1 is ready to merge (deps all merged/none, PR open, base=master)
    // autonomous_merge=true: no operator gate, merge is allowed automatically.
    let action = decide_next_action(&views, true, &std::collections::HashSet::new());

    // Then — decide_next_action must pick Merge for n1
    assert_eq!(
        action,
        OrchestratorAction::Merge {
            node_id: "n1".into(),
            pr_number: 1
        },
        "with n1 open and base=master and no unmerged parents, action must be Merge n1"
    );
}

#[test]
fn operator_gated_loop_waits_before_merge_when_autonomous_disabled() {
    // Given — one node: deps all merged (none), PR open, base=master — normally ready to merge.
    // With autonomous_merge=false (operator-gated mode), decide_next_action must return Wait.
    let views = vec![NodeView {
        node_id: "n1".into(),
        branch: "feature/bottom".into(),
        parent_dep_ids: vec![],
        child_session_id: Some("sid-n1".into()),
        child_state: Some(WorkflowState::new("Done")),
        child_phase: ChildPhase::PrOpen,
        pr: PrLiveStatus::Open {
            number: 1,
            base: "master".into(),
        },
    }];

    // When — called with autonomous_merge=false and no approved nodes (operator-gated mode)
    let action = decide_next_action(&views, false, &std::collections::HashSet::new());

    // Then — with gate=false (default), action must be Wait
    assert!(
        matches!(action, OrchestratorAction::Wait { .. }),
        "with operator-gated mode (no merge approval), assess must return Wait; got: {action:?}"
    );
}
