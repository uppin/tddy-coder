use crate::{WorkItem, WorkQueue};

/// A node with the stated id, parents and recorded position, and nothing else set.
fn a_node(node_id: &str, parents: &[&str], display_order: Option<u32>) -> WorkItem {
    WorkItem {
        node_id: node_id.to_string(),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        display_order,
        ..WorkItem::default()
    }
}

fn a_stack(nodes: Vec<WorkItem>) -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes,
        ..Default::default()
    }
}

#[test]
fn display_order_reads_a_fully_numbered_stack_in_its_recorded_positions() {
    // Given — a chain whose recorded reading order deliberately contradicts the DAG: the
    // operator moved the leaf above the root, and where a row reads is not what it builds on
    let stack = a_stack(vec![
        a_node("n1", &[], Some(2)),
        a_node("n2", &["n1"], Some(1)),
        a_node("n3", &["n2"], Some(0)),
    ]);

    // When
    let order = stack.display_order();

    // Then
    assert_eq!(order, vec!["n3", "n2", "n1"]);
}

#[test]
fn display_order_falls_back_to_topological_order_for_a_stack_that_records_none() {
    // Given — a plan written before display order existed, stored in the `nodes` array in
    // reverse: the array has never been ordered by anything
    let stack = a_stack(vec![
        a_node("n3", &["n2"], None),
        a_node("n2", &["n1"], None),
        a_node("n1", &[], None),
    ]);

    // When
    let order = stack.display_order();

    // Then — byte-identical to what `topo_order` returns, roots before dependents
    assert_eq!(order, stack.topo_order().unwrap());
    assert_eq!(order, vec!["n1", "n2", "n3"]);
}

#[test]
fn a_half_numbered_stack_sorts_its_numbered_rows_first_rather_than_interleaving_them() {
    // Given — a half-numbered stack, which has no coherent total order of its own.
    //
    // This is an internal invariant of the sort key, not what the operator ever sees. The only
    // production caller of `display_order` is the PR-stack recipe's `swap_with_neighbour`, which
    // runs after `assign_missing_display_order` inside the same atomic update, so a
    // half-numbered stack never reaches it. The rendered list is ordered by the web's
    // `orderWorkItems`, which deliberately diverges here: it falls back to topological order
    // *wholesale* for this input (PRD D25), because interleaving real positions with invented
    // ones can put a child above its parent. Pinned so a change to the key is a decision rather
    // than an accident.
    let stack = a_stack(vec![
        a_node("n1", &[], None),
        a_node("n2", &[], Some(0)),
        a_node("n3", &[], None),
    ]);

    // When
    let order = stack.display_order();

    // Then
    assert_eq!(order, vec!["n2", "n1", "n3"]);
}

#[test]
fn display_order_still_lists_every_node_when_the_dag_holds_a_cycle() {
    // Given — a cycle, which `topo_order` refuses outright. The two nodes are declared n2 before
    // n1, so declaration order and the sort key's final lexicographic tie-break disagree: a key
    // that had lost its declaration-order fallback would answer `n1, n2` instead.
    let stack = a_stack(vec![
        a_node("n2", &["n1"], None),
        a_node("n1", &["n2"], None),
    ]);

    // When — this is a render path: a list the operator cannot see is worse than one in an
    // imperfect order
    let order = stack.display_order();

    // Then — the tie-break degrades to where the nodes are declared, and nothing is dropped
    assert!(stack.topo_order().is_err());
    assert_eq!(order, vec!["n2", "n1"]);
}
