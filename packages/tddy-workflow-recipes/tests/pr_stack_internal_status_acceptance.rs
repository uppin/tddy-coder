//! PRD acceptance: internal PR-status derivation + agent override.
//!
//! `derive_internal_status` computes each node's action-needed signal from the assembled
//! views + live GitHub state (the same `NodeView` the old assess loop consumed). It is
//! `source = "derived"`. `reconcile_internal_status` applies the override-wins rule: a node
//! the agent has manually marked (`source = "override"`) is never clobbered by derivation.
//!
//! PRD: docs/ft/coder/pr-stacking.md § Internal PR status.

use tddy_core::changeset::PrInternalStatus;
use tddy_workflow_recipes::orchestrate_pr_stack::{
    derive_internal_status, reconcile_internal_status, ChildPhase, NodeView, PrLiveStatus,
};

/// A node view with sensible defaults — override only what a scenario cares about.
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

/// Look up the derived status kind for `node_id` in the derivation output.
fn derived_kind(views: &[NodeView], node_id: &str) -> String {
    derive_internal_status(views, "master")
        .into_iter()
        .find(|(id, _)| id == node_id)
        .unwrap_or_else(|| panic!("no derived status for {node_id}"))
        .1
        .kind
}

#[test]
fn derives_needs_repoint_when_a_parent_merged_but_the_pr_base_was_not_updated() {
    // Given — n1 is merged; n2 depends on n1 but its PR base still points at n1's branch
    let views = vec![
        a_node_view("n1", "feature/auth/token-store", &[], PrLiveStatus::Merged),
        a_node_view(
            "n2",
            "feature/auth/login-api",
            &["n1"],
            PrLiveStatus::Open {
                number: 42,
                base: "feature/auth/token-store".to_string(),
            },
        ),
    ];

    // When / Then
    assert_eq!(derived_kind(&views, "n2"), "needs-repoint");
}

#[test]
fn derives_ready_to_merge_when_the_pr_is_open_and_all_dependencies_are_merged() {
    // Given — n1 merged; n2 already repointed onto master and open
    let views = vec![
        a_node_view("n1", "feature/auth/token-store", &[], PrLiveStatus::Merged),
        a_node_view(
            "n2",
            "feature/auth/login-api",
            &["n1"],
            PrLiveStatus::Open {
                number: 42,
                base: "master".to_string(),
            },
        ),
    ];

    // When / Then
    assert_eq!(derived_kind(&views, "n2"), "ready-to-merge");
}

#[test]
fn derives_up_to_date_when_a_dependency_is_still_open() {
    // Given — n1 still open; n2 correctly based on n1's branch, nothing to do yet
    let views = vec![
        a_node_view(
            "n1",
            "feature/auth/token-store",
            &[],
            PrLiveStatus::Open {
                number: 41,
                base: "master".to_string(),
            },
        ),
        a_node_view(
            "n2",
            "feature/auth/login-api",
            &["n1"],
            PrLiveStatus::Open {
                number: 42,
                base: "feature/auth/token-store".to_string(),
            },
        ),
    ];

    // When / Then
    assert_eq!(derived_kind(&views, "n2"), "up-to-date");
}

#[test]
fn derives_merged_for_a_merged_pr() {
    // Given
    let views = vec![a_node_view(
        "n1",
        "feature/auth/token-store",
        &[],
        PrLiveStatus::Merged,
    )];

    // When / Then
    assert_eq!(derived_kind(&views, "n1"), "merged");
}

#[test]
fn an_agent_override_is_preserved_even_when_derivation_disagrees() {
    // Given — the agent has manually marked the node blocked with a note
    let existing = PrInternalStatus {
        kind: "blocked".into(),
        note: Some("waiting on API design".into()),
        source: "override".into(),
    };
    let derived = PrInternalStatus {
        kind: "ready-to-merge".into(),
        note: None,
        source: "derived".into(),
    };

    // When
    let reconciled = reconcile_internal_status(Some(&existing), derived);

    // Then — the agent's override wins and its note survives
    assert_eq!(reconciled.kind, "blocked");
    assert_eq!(reconciled.note.as_deref(), Some("waiting on API design"));
    assert_eq!(reconciled.source, "override");
}

#[test]
fn a_derived_status_replaces_a_previous_derived_status() {
    // Given — the previous status was itself derived (not an override)
    let existing = PrInternalStatus {
        kind: "needs-repoint".into(),
        note: None,
        source: "derived".into(),
    };
    let derived = PrInternalStatus {
        kind: "ready-to-merge".into(),
        note: None,
        source: "derived".into(),
    };

    // When
    let reconciled = reconcile_internal_status(Some(&existing), derived);

    // Then — derivation freely updates a derived status
    assert_eq!(reconciled.kind, "ready-to-merge");
    assert_eq!(reconciled.source, "derived");
}
