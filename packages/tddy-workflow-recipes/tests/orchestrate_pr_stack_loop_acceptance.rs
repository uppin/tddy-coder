//! PRD acceptance: orchestrate-pr-stack assess loop — decide_next_action picks Merge for bottom
//! node when ready; returns Wait in operator-gated mode.

use tddy_workflow_recipes::orchestrate_pr_stack::{
    ChildPhase, NodeView, OrchestratorAction, PrLiveStatus,
    decide_next_action,
};
use tddy_core::workflow::ids::WorkflowState;

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
            pr: PrLiveStatus::Open { number: 1, base: "master".into() },
        },
        NodeView {
            node_id: "n2".into(),
            branch: "feature/top".into(),
            parent_dep_ids: vec!["n1".into()],
            child_session_id: Some("sid-n2".into()),
            child_state: Some(WorkflowState::new("Done")),
            child_phase: ChildPhase::PrOpen,
            pr: PrLiveStatus::Open { number: 2, base: "feature/bottom".into() },
        },
    ];

    // When — n1 is ready to merge (deps all merged/none, PR open, base=master)
    let action = decide_next_action(&views);

    // Then — decide_next_action must pick Merge for n1
    assert_eq!(
        action,
        OrchestratorAction::Merge { node_id: "n1".into(), pr_number: 1 },
        "with n1 open and base=master and no unmerged parents, action must be Merge n1"
    );
}

#[test]
fn operator_gated_loop_waits_before_merge_when_autonomous_disabled() {
    // Given — same topology as above but with an operator-gate representation:
    // decide_next_action receives views AND a "gate open?" check.
    // The CURRENT API of decide_next_action takes only &[NodeView].
    // Per the plan, the gate is a Context flag. Until implemented, decide_next_action
    // always returns unimplemented! so this test also correctly panics.
    //
    // NOTE TO IMPLEMENTER: decide_next_action signature may need to accept a gate flag.
    // This test asserts that without gate approval, the action is Wait (not Merge).
    // For now: same call as above; test fails because decide_next_action is unimplemented.
    let views = vec![
        NodeView {
            node_id: "n1".into(),
            branch: "feature/bottom".into(),
            parent_dep_ids: vec![],
            child_session_id: Some("sid-n1".into()),
            child_state: Some(WorkflowState::new("Done")),
            child_phase: ChildPhase::PrOpen,
            pr: PrLiveStatus::Open { number: 1, base: "master".into() },
        },
    ];

    // When — called without merge gate approval (default gated mode)
    // Implementation must expose a way to pass gate=false. Suggested: second parameter
    // `autonomous_merge: bool`. The test will be updated when the signature is finalized.
    // For the RED phase, decide_next_action panics with unimplemented! before reaching gate logic.
    let action = decide_next_action(&views);

    // Then — with gate=false (default), action must be Wait
    assert!(
        matches!(action, OrchestratorAction::Wait { .. }),
        "with operator-gated mode (no merge approval), assess must return Wait; got: {action:?}"
    );
}
