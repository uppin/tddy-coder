//! Acceptance: the stack's *reading* order, persisted per node and moved only when asked.
//!
//! A stack has two orders that have been the same fact until now: the dependency graph (which node
//! builds on which, `parents`) and the reading order (how the operator wants the list laid out).
//! Deriving the second from the first means every merge, repoint and re-parenting silently
//! rewrites the operator's view — a row jumps under the cursor as a consequence of an event that
//! has nothing to do with it. `StackNode.display_order` separates them: it is persisted, it is
//! assigned once, and the only thing that changes it is [`move_planned_pr_node`].
//!
//! The rules pinned here are all *absences* as much as effects. Adding a node appends and renumbers
//! nothing above it. Deleting a node leaves the survivors' numbers alone — a gap is harmless in a
//! sort key, and closing it would move rows the operator did not touch. A move swaps exactly one
//! pair and leaves `parents` untouched, which is the whole point of the field existing. Only a
//! wholesale re-seed, which replaces the plan itself, renumbers everything.
//!
//! A stack authored before this feature carries no numbers at all. Every mutator normalizes on
//! write, so the *next* write — any write, including one about something else entirely — numbers
//! every node in topological order, and the numbers on disk are then the truth from that point on.
//! Topological order, not `Vec` order: the `nodes` array has never been ordered by anything.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-08-01-pr-stack-panel-ux.md` § C3 (D24, D25).
//! Changeset: `docs/dev/1-WIP/CS-2026-08-01-pr-stack-panel-ux.md` (Milestone 1).

mod common;

use std::path::Path;

use common::{a_planned_node, assert_rejected, node_ids, parents_of, stack_of, write_stack};
use tddy_core::changeset::StackNode;
use tddy_workflow_recipes::plan_pr_stack::{PlannedPr, StackPlanOutput};
use tddy_workflow_recipes::pr_stack::{
    add_planned_pr_node, adopt_pr_as_stack_node, delete_planned_pr_node, move_planned_pr_node,
    reseed_stack_from_plan_if_unspawned, update_planned_pr_node, AddPlannedPrInput, AdoptedPrFacts,
    UpdatePlannedPrInput,
};

const UP: &str = "up";
const DOWN: &str = "down";

// --- builders ---------------------------------------------------------------

/// The same node, at a stated position in the reading order.
fn at_order(node: StackNode, display_order: u32) -> StackNode {
    StackNode {
        display_order: Some(display_order),
        ..node
    }
}

/// The same node as a plan written before this feature existed recorded it: no position at all.
fn unnumbered(node: StackNode) -> StackNode {
    StackNode {
        display_order: None,
        ..node
    }
}

fn an_added_pr(title: &str, parents: &[&str]) -> AddPlannedPrInput {
    AddPlannedPrInput {
        title: title.to_string(),
        description: format!("{title} description"),
        branch_suggestion: Some(format!("feature/stack/{title}")),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        child_recipe: None,
    }
}

fn an_existing_pr(pull_number: u64, head_branch: &str) -> AdoptedPrFacts {
    AdoptedPrFacts {
        pull_number,
        title: format!("PR {pull_number}"),
        body: format!("body of PR {pull_number}"),
        head_branch: head_branch.to_string(),
        url: format!("https://github.com/acme/repo/pull/{pull_number}"),
        phase: "open".to_string(),
    }
}

/// A plan whose `prs` array order is the reading order a re-seed adopts wholesale.
fn a_plan(prs: Vec<PlannedPr>) -> StackPlanOutput {
    StackPlanOutput {
        version: 2,
        exploration: None,
        prs,
    }
}

fn a_planned_pr(node_id: &str, parents: &[&str]) -> PlannedPr {
    PlannedPr {
        node_id: node_id.to_string(),
        title: format!("{node_id} title"),
        description: format!("{node_id} description"),
        branch_suggestion: Some(format!("feature/stack/{node_id}")),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        child_recipe: None,
    }
}

// --- reading the order back -------------------------------------------------

/// The persisted position of each named node, in the order named — so an assertion states the whole
/// order as one literal rather than one `assert_eq!` per row.
fn orders_of(dir: &Path, node_ids: &[&str]) -> Vec<Option<u32>> {
    let stack = stack_of(dir);
    node_ids
        .iter()
        .map(|id| {
            stack
                .node(id)
                .unwrap_or_else(|| panic!("the stack on disk holds no node '{id}'"))
                .display_order
        })
        .collect()
}

fn order_of(dir: &Path, node_id: &str) -> Option<u32> {
    orders_of(dir, &[node_id])[0]
}

// --- tests ------------------------------------------------------------------

#[test]
fn an_added_planned_pr_lands_at_the_bottom_of_the_order() {
    // Given — a two-row stack, numbered 0 and 1
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
        ],
    );

    // When
    let added = add_planned_pr_node(dir, an_added_pr("a third PR", &["n2"]))
        .expect("appending a planned PR should succeed");

    // Then — it reads below both, never between them
    assert_eq!(order_of(dir, &added.node_id), Some(2));
}

#[test]
fn adding_a_planned_pr_never_renumbers_the_rows_above_it() {
    // Given — a stack whose numbering already has a gap in it, left by an earlier delete
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 3),
            at_order(a_planned_node("n2", &["n1"]), 7),
        ],
    );

    // When
    let added = add_planned_pr_node(dir, an_added_pr("a third PR", &["n2"]))
        .expect("appending a planned PR should succeed");

    // Then — the new row takes one past the highest, and the gap above it is left exactly as it was
    assert_eq!(
        orders_of(dir, &["n1", "n2", &added.node_id]),
        vec![Some(3), Some(7), Some(8)],
        "an append must number from the highest existing position, not from the row count"
    );
}

#[test]
fn an_adopted_pr_lands_at_the_bottom_of_the_order() {
    // Given — a two-row stack, and a pull request the stack does not yet track
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
        ],
    );

    // When
    let adopted = adopt_pr_as_stack_node(
        dir,
        an_existing_pr(77, "feature/stack/adopted"),
        vec!["n2".to_string()],
    )
    .expect("adopting a PR should succeed");

    // Then — an adopted node is appended like any other, and nothing above it moves
    assert_eq!(
        orders_of(dir, &["n1", "n2", &adopted.node_id]),
        vec![Some(0), Some(1), Some(2)]
    );
}

#[test]
fn a_legacy_stack_is_numbered_on_the_next_write_in_topological_order() {
    // Given — a chain n1 <- n2 <- n3 written before display order existed, and stored in the `nodes`
    // array in reverse: the array has never been ordered by anything, so it must not be the source
    // the numbering is taken from.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            unnumbered(a_planned_node("n3", &["n2"])),
            unnumbered(a_planned_node("n2", &["n1"])),
            unnumbered(a_planned_node("n1", &[])),
        ],
    );

    // When — a write about something else entirely: an edit to one node's title
    update_planned_pr_node(
        dir,
        UpdatePlannedPrInput {
            node_id: "n2".to_string(),
            title: Some("a clearer title".to_string()),
            description: None,
            branch_suggestion: None,
        },
    )
    .expect("editing a title should succeed");

    // Then — every node is numbered, roots before dependents
    assert_eq!(
        orders_of(dir, &["n1", "n2", "n3"]),
        vec![Some(0), Some(1), Some(2)],
        "a legacy stack must be numbered in topological order, not in `nodes` array order"
    );
    // …and the edit the caller actually asked for landed
    assert_eq!(stack_of(dir).node("n2").unwrap().title, "a clearer title");
}

#[test]
fn a_legacy_stack_of_independent_rows_is_numbered_in_the_order_they_were_declared() {
    // Given — a legacy plan of three rows that depend on nothing, declared n1, n2, n10. Every one of
    // them is ready at once, and the web places ready rows in declaration order — so `n1, n2, n10`
    // is the list the operator has been looking at all along. Sorted by node id it reads `n1, n10,
    // n2`, which is what a lexicographic tie-break would freeze into the numbers.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            unnumbered(a_planned_node("n1", &[])),
            unnumbered(a_planned_node("n2", &[])),
            unnumbered(a_planned_node("n10", &[])),
        ],
    );

    // When — a write about something else entirely
    update_planned_pr_node(
        dir,
        UpdatePlannedPrInput {
            node_id: "n2".to_string(),
            title: Some("a clearer title".to_string()),
            description: None,
            branch_suggestion: None,
        },
    )
    .expect("editing a title should succeed");

    // Then — the numbers agree with the list that was rendered, so the operator's first "Move up"
    // swaps the pair they can actually see side by side
    assert_eq!(
        orders_of(dir, &["n1", "n2", "n10"]),
        vec![Some(0), Some(1), Some(2)],
        "rows that are ready at the same time must be numbered in declaration order"
    );
}

#[test]
fn deleting_a_row_leaves_the_orders_of_the_survivors_alone() {
    // Given — three numbered rows
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
            at_order(a_planned_node("n3", &["n2"]), 2),
        ],
    );

    // When — the middle one is removed from the plan
    delete_planned_pr_node(dir, "n2").expect("deleting a planned node should succeed");

    // Then — the survivors keep the positions they had. A gap is harmless in a sort key; closing it
    // would move a row the operator never touched.
    assert_eq!(orders_of(dir, &["n1", "n3"]), vec![Some(0), Some(2)]);
    assert_eq!(node_ids(dir), vec!["n1".to_string(), "n3".to_string()]);
}

#[test]
fn reseeding_an_unspawned_plan_renumbers_from_the_new_plan_order() {
    // Given — a two-row stack, neither node spawned, numbered 0 and 1
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
        ],
    );

    // When — the agent re-emits the plan with the two entries the other way round in the array,
    // while the dependency between them is unchanged
    reseed_stack_from_plan_if_unspawned(
        dir,
        &a_plan(vec![a_planned_pr("n2", &["n1"]), a_planned_pr("n1", &[])]),
    )
    .expect("re-seeding an unspawned stack should succeed");

    // Then — a re-seed replaces the plan, so it replaces the reading order too, and it takes the
    // plan's array order rather than re-deriving one from the parents
    assert_eq!(
        orders_of(dir, &["n2", "n1"]),
        vec![Some(0), Some(1)],
        "a re-seeded stack must read in the new plan's array order"
    );
}

#[test]
fn moving_a_row_up_swaps_it_with_the_row_above_it() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
            at_order(a_planned_node("n3", &["n2"]), 2),
        ],
    );

    // When
    move_planned_pr_node(dir, "n3", UP).expect("moving a row up should succeed");

    // Then — exactly one pair swapped
    assert_eq!(
        orders_of(dir, &["n1", "n2", "n3"]),
        vec![Some(0), Some(2), Some(1)]
    );
}

#[test]
fn moving_a_row_down_swaps_it_with_the_row_below_it() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
            at_order(a_planned_node("n3", &["n2"]), 2),
        ],
    );

    // When
    move_planned_pr_node(dir, "n1", DOWN).expect("moving a row down should succeed");

    // Then
    assert_eq!(
        orders_of(dir, &["n1", "n2", "n3"]),
        vec![Some(1), Some(0), Some(2)]
    );
}

#[test]
fn moving_the_first_row_up_changes_nothing() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
        ],
    );

    // When — moving past the top edge is the control being inert, not the caller being wrong
    move_planned_pr_node(dir, "n1", UP).expect("moving past the top edge should succeed");

    // Then
    assert_eq!(orders_of(dir, &["n1", "n2"]), vec![Some(0), Some(1)]);
}

#[test]
fn moving_the_last_row_down_changes_nothing() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
        ],
    );

    // When
    move_planned_pr_node(dir, "n2", DOWN).expect("moving past the bottom edge should succeed");

    // Then
    assert_eq!(orders_of(dir, &["n1", "n2"]), vec![Some(0), Some(1)]);
}

#[test]
fn moving_a_row_leaves_the_dependency_graph_untouched() {
    // Given — a chain, so a reordering that touched `parents` would be visible
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
            at_order(a_planned_node("n3", &["n2"]), 2),
        ],
    );

    // When — n3 is read above n2, while still building on it
    move_planned_pr_node(dir, "n3", UP).expect("moving a row up should succeed");

    // Then — the reading order moved and the DAG did not. This is the whole reason the field exists:
    // where a row reads and what it builds on are independent facts.
    assert_eq!(
        orders_of(dir, &["n1", "n2", "n3"]),
        vec![Some(0), Some(2), Some(1)]
    );
    assert_eq!(parents_of(dir, "n1"), Vec::<String>::new());
    assert_eq!(parents_of(dir, "n2"), vec!["n1".to_string()]);
    assert_eq!(parents_of(dir, "n3"), vec!["n2".to_string()]);
}

#[test]
fn moving_a_row_in_a_direction_that_is_neither_up_nor_down_is_refused_without_writing() {
    // Given — a stack whose order a bad direction must not disturb
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
        ],
    );

    // When — a direction no control on the panel sends: a stale client, or a caller that is not the
    // web at all
    let result = move_planned_pr_node(dir, "n2", "sideways");

    // Then — refused by name rather than silently treated as one of the two real directions, which
    // would move a row in a direction nobody asked for
    assert_rejected(result).with_reason_containing("sideways");
    assert_eq!(orders_of(dir, &["n1", "n2"]), vec![Some(0), Some(1)]);
}

#[test]
fn moving_an_unknown_node_is_refused_without_writing() {
    // Given — a stack with no node called n9
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            at_order(a_planned_node("n1", &[]), 0),
            at_order(a_planned_node("n2", &["n1"]), 1),
        ],
    );

    // When
    let result = move_planned_pr_node(dir, "n9", UP);

    // Then — refused by name, and the order on disk is exactly what it was
    assert_rejected(result).with_reason_containing("n9");
    assert_eq!(orders_of(dir, &["n1", "n2"]), vec![Some(0), Some(1)]);
    assert_eq!(node_ids(dir), vec!["n1".to_string(), "n2".to_string()]);
}
