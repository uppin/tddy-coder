//! The operator's reading order: `display_order` numbering and the one-step move.

use std::path::Path;

use tddy_core::changeset::StackNode;

/// Number every node that carries no [`StackNode::display_order`], appending them after the highest
/// position already recorded, in topological order.
///
/// Idempotent: a stack whose nodes are all numbered comes out unchanged, so every mutator can call
/// it unconditionally as its first act. That is what makes a stack written before the field existed
/// self-healing — the *next* write, about whatever it is about, records a total reading order, and
/// [`tddy_core::changeset::Stack::display_order`]'s topological fallback stops applying to it.
///
/// Topological order, not `nodes` array order: the array has never been ordered by anything, so
/// numbering from it would freeze an arbitrary sequence into the operator's view. Existing numbers
/// are never rewritten — a gap left by a delete is harmless in a sort key, and closing it would move
/// rows nobody touched.
pub fn assign_missing_display_order(stack: &mut tddy_core::changeset::Stack) {
    let mut next = next_display_order(stack);
    for node_id in numbering_order(stack) {
        let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) else {
            continue;
        };
        if node.display_order.is_none() {
            node.display_order = Some(next);
            next = next.saturating_add(1);
        }
    }
}

/// The order the unnumbered nodes are numbered in: topological, with **declaration order** — the
/// position in `Stack.nodes` — as the tie-break between nodes that are ready at the same time.
///
/// The tie-break is the whole reason this is not [`tddy_core::changeset::Stack::topo_order`], whose
/// Kahn queue is seeded from a `BTreeMap` and so breaks ties by node id *lexicographically*: ten
/// independent nodes `n1…n10` come out `n1, n10, n2, …`. The web's `topoSortStackNodes` places every
/// ready node in declaration order, so that is the list a legacy plan was actually rendered as, and
/// the first number written must agree with it — otherwise the operator's first "Move up" swaps a
/// pair they never saw side by side and the list reshuffles under them.
fn numbering_order(stack: &tddy_core::changeset::Stack) -> Vec<String> {
    let known: std::collections::HashSet<&str> =
        stack.nodes.iter().map(|n| n.node_id.as_str()).collect();
    let mut placed: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut remaining: Vec<&StackNode> = stack.nodes.iter().collect();
    let mut ordered: Vec<String> = Vec::with_capacity(stack.nodes.len());

    while !remaining.is_empty() {
        let (ready, blocked): (Vec<&StackNode>, Vec<&StackNode>) =
            remaining.iter().partition(|node| {
                node.parents
                    .iter()
                    .all(|p| !known.contains(p.as_str()) || placed.contains(p.as_str()))
            });
        if ready.is_empty() {
            // A cycle: every remaining node waits on another. Declaration order is an order the
            // caller can render, which beats dropping rows or returning none at all.
            ordered.extend(blocked.iter().map(|node| node.node_id.clone()));
            break;
        }
        for node in &ready {
            placed.insert(node.node_id.as_str());
            ordered.push(node.node_id.clone());
        }
        remaining = blocked;
    }
    ordered
}

/// One past the highest position any node records, or `0` for a stack that records none.
pub(super) fn next_display_order(stack: &tddy_core::changeset::Stack) -> u32 {
    stack
        .nodes
        .iter()
        .filter_map(|node| node.display_order)
        .max()
        .map_or(0, |highest| highest.saturating_add(1))
}

/// Move one node one position up or down the operator's reading order, swapping positions with the
/// neighbour it passes.
///
/// `direction` is `"up"` or `"down"`. Moving past either end is a **successful no-op**: the control
/// at the end of the list is inert, not wrong, and refusing would make the operator's click an error
/// they have to read.
///
/// Touches nothing but `display_order` — `parents` is the dependency graph and this is the reading
/// order, and the whole point of the field is that the two move independently. An unknown node id is
/// refused before anything is written.
pub fn move_planned_pr_node(
    session_dir: &Path,
    node_id: &str,
    direction: &str,
) -> Result<tddy_core::changeset::Stack, String> {
    use tddy_core::changeset::{read_changeset, update_stack_atomic};
    const OP: &str = "move_planned_pr_node";

    let up = match direction {
        "up" => true,
        "down" => false,
        other => {
            return Err(format!(
                "{OP}: unknown direction '{other}' — expected 'up' or 'down'"
            ))
        }
    };

    let stack = read_changeset(session_dir)
        .map_err(|e| format!("{OP}: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    if stack.node(node_id).is_none() {
        return Err(format!("{OP}: node '{node_id}' not found"));
    }

    update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        swap_with_neighbour(stack, node_id, up);
    })
    .map_err(|e| format!("{OP}: failed to write stack: {e}"))?;

    read_changeset(session_dir)
        .map_err(|e| format!("{OP}: failed to reload stack: {e}"))
        .map(|changeset| changeset.stack.unwrap_or_default())
}

/// Swap `node_id`'s position with the row above (`up`) or below it, or leave the stack alone when
/// there is no such row. Every node is numbered by the time this runs.
fn swap_with_neighbour(stack: &mut tddy_core::changeset::Stack, node_id: &str, up: bool) {
    let order = stack.display_order();
    let Some(position) = order.iter().position(|id| id == node_id) else {
        return;
    };
    // `get` answers `None` past either end, which is the no-op the control at the edge of the list
    // relies on.
    let neighbour_index = if up {
        position.checked_sub(1)
    } else {
        position.checked_add(1)
    };
    let Some(neighbour) = neighbour_index.and_then(|index| order.get(index)).cloned() else {
        return;
    };

    let position_of = |id: &str| stack.node(id).and_then(|node| node.display_order);
    let (Some(moved), Some(displaced)) = (position_of(node_id), position_of(&neighbour)) else {
        return;
    };
    for node in &mut stack.nodes {
        if node.node_id == node_id {
            node.display_order = Some(displaced);
        } else if node.node_id == neighbour {
            node.display_order = Some(moved);
        }
    }
}
