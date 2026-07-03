//! Unit-level coverage for `derive_internal_status`: the full decision table evaluated in one
//! pass (proving the kinds coexist across a mixed DAG) and the invariant that every derived
//! result is stamped `source = "derived"`.
//!
//! PRD: docs/ft/coder/pr-stacking.md § Internal PR status.

use std::collections::HashMap;

use tddy_workflow_recipes::orchestrate_pr_stack::{
    derive_internal_status, ChildPhase, NodeView, PrLiveStatus,
};

fn a_node_view(node_id: &str, branch: &str, parents: &[&str], pr: PrLiveStatus) -> NodeView {
    NodeView {
        node_id: node_id.to_string(),
        branch: branch.to_string(),
        parent_dep_ids: parents.iter().map(|p| p.to_string()).collect(),
        child_session_id: Some(format!("sid-{node_id}")),
        child_state: None,
        child_phase: ChildPhase::PrOpen,
        pr,
    }
}

/// A mixed DAG that exercises every derived kind at once.
fn a_mixed_stack() -> Vec<NodeView> {
    vec![
        // merged root
        a_node_view("n0", "feature/n0", &[], PrLiveStatus::Merged),
        // open root, deps vacuously merged, based on master
        a_node_view(
            "n1",
            "feature/n1",
            &[],
            PrLiveStatus::Open {
                number: 1,
                base: "master".into(),
            },
        ),
        // parent n0 merged but base still points at n0's branch
        a_node_view(
            "n2",
            "feature/n2",
            &["n0"],
            PrLiveStatus::Open {
                number: 2,
                base: "feature/n0".into(),
            },
        ),
        // parent n1 still open; correctly based on n1's branch
        a_node_view(
            "n3",
            "feature/n3",
            &["n1"],
            PrLiveStatus::Open {
                number: 3,
                base: "feature/n1".into(),
            },
        ),
    ]
}

#[test]
fn covers_the_full_decision_table_in_one_pass() {
    // Given
    let views = a_mixed_stack();

    // When
    let derived: HashMap<String, String> = derive_internal_status(&views, "master")
        .into_iter()
        .map(|(id, s)| (id, s.kind))
        .collect();

    // Then
    assert_eq!(derived["n0"], "merged");
    assert_eq!(derived["n1"], "ready-to-merge");
    assert_eq!(derived["n2"], "needs-repoint");
    assert_eq!(derived["n3"], "up-to-date");
}

#[test]
fn stamps_every_derived_result_with_source_derived() {
    // Given
    let views = a_mixed_stack();

    // When
    let derived = derive_internal_status(&views, "master");

    // Then — derivation output is always `source = "derived"` (never accidentally "override")
    for (id, status) in derived {
        assert_eq!(
            status.source, "derived",
            "node {id} must be stamped derived"
        );
    }
}
