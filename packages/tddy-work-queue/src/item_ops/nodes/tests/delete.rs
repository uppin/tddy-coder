use crate::item_ops::{
    delete_work_item,
    nodes::tests::parents_of,
    test_support::{a_node, a_started_node, assert_rejected, node_ids, stack_on_disk, write_stack},
};
use rstest::rstest;

use crate::item_ops::test_support::a_stack_session_dir;

#[test]
fn deleting_a_middle_node_reparents_its_children_onto_that_nodes_parents() {
    // Given — a chain n1 → n2 → n3 where n2 turned out to be unnecessary
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            a_node("n1", "one", vec![]),
            a_node("n2", "two", vec!["n1"]),
            a_node("n3", "three", vec!["n2"]),
        ],
    );

    // When
    let deleted = delete_work_item(dir, "n2").expect("deleting a planned node should succeed");

    // Then — n3 inherits n1, so the chain stays connected
    assert_eq!(deleted.node.node_id, "n2");
    assert_eq!(deleted.reparented_children, vec!["n3".to_string()]);
    assert_eq!(node_ids(dir), vec!["n1", "n3"]);
    assert_eq!(parents_of(dir, "n3"), vec!["n1".to_string()]);
}

#[test]
fn deleting_a_root_node_leaves_its_children_as_roots() {
    // Given — n1 is a root and n2 sits on it
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![a_node("n1", "one", vec![]), a_node("n2", "two", vec!["n1"])],
    );

    // When
    delete_work_item(dir, "n1").expect("deleting a root should succeed");

    // Then — n2 becomes a root itself, basing off the stack bottom
    assert_eq!(node_ids(dir), vec!["n2"]);
    assert_eq!(parents_of(dir, "n2"), Vec::<String>::new());
}

#[test]
fn a_child_that_already_lists_the_inherited_parent_does_not_gain_a_duplicate() {
    // Given — a diamond: n3 depends on both n1 and n2, and n2 itself depends on n1
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            a_node("n1", "one", vec![]),
            a_node("n2", "two", vec!["n1"]),
            a_node("n3", "three", vec!["n1", "n2"]),
        ],
    );

    // When — n2 is removed, so n3 would inherit n1 it already has
    delete_work_item(dir, "n2").expect("deleting a diamond's middle should succeed");

    // Then — one edge to n1, not two
    assert_eq!(parents_of(dir, "n3"), vec!["n1".to_string()]);
}

#[test]
fn no_reference_to_the_deleted_node_survives_in_any_nodes_parents() {
    // Given — two separate children both depend on n2
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            a_node("n1", "one", vec![]),
            a_node("n2", "two", vec!["n1"]),
            a_node("n3", "three", vec!["n2"]),
            a_node("n4", "four", vec!["n2"]),
        ],
    );

    // When
    delete_work_item(dir, "n2").expect("deleting should succeed");

    // Then — a dangling parent ref would be silently ignored by `topo_order`, so none may remain
    let dangling: Vec<String> = stack_on_disk(dir)
        .nodes
        .iter()
        .filter(|n| n.parents.iter().any(|p| p == "n2"))
        .map(|n| n.node_id.clone())
        .collect();
    assert_eq!(dangling, Vec::<String>::new());
    assert_eq!(parents_of(dir, "n3"), vec!["n1".to_string()]);
    assert_eq!(parents_of(dir, "n4"), vec!["n1".to_string()]);
}

#[test]
fn deleting_a_node_whose_pr_is_open_is_rejected_and_the_stack_on_disk_is_unchanged() {
    // Given — n1's PR is open on GitHub
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![a_started_node("n1", "feature/n1", "open", vec![])],
    );

    // When
    let result = delete_work_item(dir, "n1");

    // Then — closing a PR is externally visible and must be asked for explicitly
    assert_rejected(result).with_reason_containing("open");
    assert_eq!(node_ids(dir), vec!["n1"]);
}

#[rstest]
#[case::merged("merged")]
#[case::closed("closed")]
#[case::errored("error")]
fn deleting_a_node_whose_pr_is_no_longer_open_is_allowed(#[case] phase: &str) {
    // Given — a node whose PR has already left the open state
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(dir, vec![a_started_node("n1", "feature/n1", phase, vec![])]);

    // When
    delete_work_item(dir, "n1").expect("a node with no open PR should delete");

    // Then
    assert_eq!(node_ids(dir), Vec::<String>::new());
}

#[test]
fn deleting_a_started_node_reports_its_orphaned_branch_and_session_id() {
    // Given — n1 owns a branch and a child session, and its PR is already merged
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![a_started_node("n1", "feature/n1", "merged", vec![])],
    );

    // When
    let deleted = delete_work_item(dir, "n1").expect("deleting should succeed");

    // Then — deletion is a plan operation, so what it left unowned is named, not removed
    assert_eq!(deleted.orphaned_branch.as_deref(), Some("feature/n1"));
    assert_eq!(
        deleted.orphaned_session_id.as_deref(),
        Some("session-for-n1")
    );
}

#[test]
fn deleting_an_unknown_node_id_is_rejected() {
    // Given
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(dir, vec![a_node("n1", "one", vec![])]);

    // When
    let result = delete_work_item(dir, "n9");

    // Then
    assert_rejected(result).with_reason_containing("n9");
    assert_eq!(node_ids(dir), vec!["n1"]);
}
