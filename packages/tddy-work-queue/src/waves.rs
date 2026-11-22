//! Waves, the derived line and readiness (`#workqueue` 10/14), over a queue's dependency edges
//! (`#workqueue` 9/14). Pure: nothing is stored; everything is derived from the item order and the
//! edges.
//!
//! - **Waves** — wave 1 is every item with no need; wave k every item whose needs are all in waves
//!   below k. An item with no needs is wave 1 wherever it sits.
//! - **Derived line** — the waves concatenated; inside a wave, most transitive dependents first, ties
//!   in the queue's own order.
//! - **Readiness** — an item's *binding* needs: the unmet ones in the wave immediately below it.
//! - **Line check** — whether a branch line steps back a wave, and where first.

use crate::lineage::{DependencyEdge, LineageError};

/// Whether a line keeps the waves non-decreasing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineCheck {
    InOrder,
    /// `first` sits in a lower wave than the item before it.
    OutOfOrder {
        first: String,
    },
}

/// The waves of `items` (queue order) under `edges`; a cycle is reported instead.
pub fn waves(items: &[String], edges: &[DependencyEdge]) -> Result<Vec<Vec<String>>, LineageError> {
    // TODO(n10 waves-line): implement
    let _ = (items, edges);
    todo!("TODO(n10): implement waves")
}

/// The derived line: waves concatenated, blockers first inside each wave, ties in queue order.
pub fn derive_line(
    items: &[String],
    edges: &[DependencyEdge],
) -> Result<Vec<String>, LineageError> {
    // TODO(n10 waves-line): implement
    let _ = (items, edges);
    todo!("TODO(n10): implement derive_line")
}

/// Every item that needs `item`, directly or through others.
pub fn transitive_dependents(edges: &[DependencyEdge], item: &str) -> Vec<String> {
    // TODO(n10 waves-line): implement
    let _ = (edges, item);
    todo!("TODO(n10): implement transitive_dependents")
}

/// `item`'s binding needs: those in the wave immediately below it that are not in `done`.
pub fn readiness(
    items: &[String],
    edges: &[DependencyEdge],
    item: &str,
    done: &[String],
) -> Result<Vec<String>, LineageError> {
    // TODO(n10 waves-line): implement
    let _ = (items, edges, item, done);
    todo!("TODO(n10): implement readiness")
}

/// Whether `line` (bottom first) keeps the waves non-decreasing.
pub fn line_check(
    items: &[String],
    edges: &[DependencyEdge],
    line: &[String],
) -> Result<LineCheck, LineageError> {
    // TODO(n10 waves-line): implement
    let _ = (items, edges, line);
    todo!("TODO(n10): implement line_check")
}
