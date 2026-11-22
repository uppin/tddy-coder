//! A work queue's lineage (`#workqueue` 9/14): line endpoints from PR bases or planned bases, stored
//! dependency edges, cycles refused.

use pretty_assertions::assert_eq;
use tddy_core::changeset::{WorkQueue, WorkItem};
use tddy_work_queue::lineage::{
    dependencies_of, line_endpoint_of_base, planned_line_parent, set_dependencies, DependencyEdge,
    LineEndpoint, LineageError,
};

const TRUNK: &str = "master";

fn an_item(id: &str, branch: &str, parents: &[&str]) -> WorkItem {
    WorkItem {
        node_id: id.to_string(),
        branch: Some(branch.to_string()),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..WorkItem::default()
    }
}

/// n1 (feat/one) ← n2 (feat/two) ← n3 (feat/three), planned as a line.
fn a_queue() -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: vec![
            an_item("n1", "feat/one", &[]),
            an_item("n2", "feat/two", &["n1"]),
            an_item("n3", "feat/three", &["n2"]),
        ],
    }
}

fn an_edge(from: &str, to: &str) -> DependencyEdge {
    DependencyEdge {
        from: from.to_string(),
        to: to.to_string(),
    }
}

#[test]
fn a_pr_based_on_another_items_branch_has_a_line_edge_from_that_item() {
    assert_eq!(
        line_endpoint_of_base(&a_queue(), "feat/one", TRUNK),
        LineEndpoint::Item("n1".to_string())
    );
}

#[test]
fn a_pr_based_on_the_trunk_has_a_trunk_endpoint() {
    assert_eq!(
        line_endpoint_of_base(&a_queue(), "master", TRUNK),
        LineEndpoint::Trunk("master".to_string())
    );
}

#[test]
fn a_pr_based_on_a_branch_outside_the_queue_has_an_external_endpoint() {
    assert_eq!(
        line_endpoint_of_base(&a_queue(), "feat/elsewhere", TRUNK),
        LineEndpoint::External("feat/elsewhere".to_string())
    );
}

#[test]
fn an_item_without_a_pr_shows_its_planned_base() {
    assert_eq!(
        (
            planned_line_parent(&a_queue(), "n2", TRUNK),
            planned_line_parent(&a_queue(), "n1", TRUNK),
        ),
        (
            Ok(LineEndpoint::Item("n1".to_string())),
            Ok(LineEndpoint::Trunk("master".to_string()))
        )
    );
}

#[test]
fn dependency_edges_are_stored_on_the_queue() {
    // Given
    let mut queue = a_queue();

    // When — n3 needs n1's behaviour only (its line parent n2 is incidental)
    set_dependencies(&mut queue, "n3", vec!["n1".to_string()]).expect("set");

    // Then
    assert_eq!(dependencies_of(&queue), vec![an_edge("n1", "n3")]);
}

#[test]
fn a_dependency_set_closing_a_cycle_is_refused_naming_the_cycle() {
    // Given
    let mut queue = a_queue();
    set_dependencies(&mut queue, "n2", vec!["n1".to_string()]).expect("n2 needs n1");
    set_dependencies(&mut queue, "n3", vec!["n2".to_string()]).expect("n3 needs n2");

    // When
    let refused = set_dependencies(&mut queue, "n1", vec!["n3".to_string()]);

    // Then
    assert_eq!(
        refused,
        Err(LineageError::Cycle(vec![
            "n1".to_string(),
            "n2".to_string(),
            "n3".to_string(),
            "n1".to_string()
        ]))
    );
    assert_eq!(
        dependencies_of(&queue),
        vec![an_edge("n1", "n2"), an_edge("n2", "n3")]
    );
}

#[test]
fn a_dependency_on_an_unknown_item_is_refused() {
    // Given
    let mut queue = a_queue();

    // When
    let refused = set_dependencies(&mut queue, "n2", vec!["n9".to_string()]);

    // Then
    assert_eq!(refused, Err(LineageError::NoSuchItem("n9".to_string())));
}
