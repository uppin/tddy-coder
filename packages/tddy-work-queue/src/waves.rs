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

use std::collections::{BTreeSet, HashMap, HashSet};

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
///
/// Edges naming an id outside `items` are ignored: they are not this queue's.
pub fn waves(items: &[String], edges: &[DependencyEdge]) -> Result<Vec<Vec<String>>, LineageError> {
    let known: HashSet<&str> = items.iter().map(String::as_str).collect();
    let mut needs: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges {
        if known.contains(edge.from.as_str()) && known.contains(edge.to.as_str()) {
            needs
                .entry(edge.to.as_str())
                .or_default()
                .push(edge.from.as_str());
        }
    }

    let mut placed: HashSet<&str> = HashSet::new();
    let mut remaining: Vec<&str> = items.iter().map(String::as_str).collect();
    let mut result: Vec<Vec<String>> = Vec::new();
    while !remaining.is_empty() {
        let (ready, blocked): (Vec<&str>, Vec<&str>) = remaining.iter().partition(|item| {
            needs
                .get(**item)
                .is_none_or(|from| from.iter().all(|need| placed.contains(need)))
        });
        if ready.is_empty() {
            return Err(LineageError::Cycle(cycle_among(&blocked, &needs)));
        }
        placed.extend(ready.iter().copied());
        result.push(ready.into_iter().map(str::to_string).collect());
        remaining = blocked;
    }
    Ok(result)
}

/// A cycle among `stuck` items (none of which could be placed): walk needs that are themselves stuck
/// until an item repeats. Returned first repeated last, like `LineageError::Cycle`.
// TODO(n9 lineage-graph): the queue's own cycle check, `lineage::detect_cycle`, is n9's; reach it
// through that once it lands. This only names what stopped the placement.
fn cycle_among(stuck: &[&str], needs: &HashMap<&str, Vec<&str>>) -> Vec<String> {
    let stuck_set: HashSet<&str> = stuck.iter().copied().collect();
    let mut path: Vec<&str> = Vec::new();
    let mut current = stuck[0];
    loop {
        if let Some(at) = path.iter().position(|seen| *seen == current) {
            let mut cycle: Vec<String> = path[at..].iter().map(|s| s.to_string()).collect();
            cycle.push(current.to_string());
            return cycle;
        }
        path.push(current);
        // Every stuck item has at least one stuck need, or it would have been placed.
        current = needs
            .get(current)
            .and_then(|from| from.iter().find(|need| stuck_set.contains(*need)))
            .copied()
            .expect("a stuck item has a stuck need");
    }
}

/// The derived line: waves concatenated, blockers first inside each wave, ties in queue order.
pub fn derive_line(
    items: &[String],
    edges: &[DependencyEdge],
) -> Result<Vec<String>, LineageError> {
    let mut line = Vec::with_capacity(items.len());
    for mut wave in waves(items, edges)? {
        // Stable: equal counts keep the queue's order.
        wave.sort_by_key(|item| std::cmp::Reverse(transitive_dependents(edges, item).len()));
        line.extend(wave);
    }
    Ok(line)
}

/// Every item that needs `item`, directly or through others, sorted.
pub fn transitive_dependents(edges: &[DependencyEdge], item: &str) -> Vec<String> {
    let mut found: BTreeSet<&str> = BTreeSet::new();
    let mut frontier = vec![item];
    while let Some(current) = frontier.pop() {
        for edge in edges.iter().filter(|edge| edge.from == current) {
            if edge.to != item && found.insert(edge.to.as_str()) {
                frontier.push(edge.to.as_str());
            }
        }
    }
    found.into_iter().map(str::to_string).collect()
}

/// `item`'s binding needs: those in the wave immediately below it that are not in `done`.
pub fn readiness(
    items: &[String],
    edges: &[DependencyEdge],
    item: &str,
    done: &[String],
) -> Result<Vec<String>, LineageError> {
    let layers = waves(items, edges)?;
    let own =
        wave_index(&layers, item).ok_or_else(|| LineageError::NoSuchItem(item.to_string()))?;
    let Some(below) = own.checked_sub(1).map(|below| &layers[below]) else {
        return Ok(Vec::new());
    };
    let needed: HashSet<&str> = edges
        .iter()
        .filter(|edge| edge.to == item)
        .map(|edge| edge.from.as_str())
        .collect();
    Ok(below
        .iter()
        .filter(|candidate| needed.contains(candidate.as_str()) && !done.contains(candidate))
        .cloned()
        .collect())
}

/// Whether `line` (bottom first) keeps the waves non-decreasing.
pub fn line_check(
    items: &[String],
    edges: &[DependencyEdge],
    line: &[String],
) -> Result<LineCheck, LineageError> {
    let layers = waves(items, edges)?;
    let mut previous = 0;
    for entry in line {
        let own = wave_index(&layers, entry)
            .ok_or_else(|| LineageError::NoSuchItem(entry.to_string()))?;
        if own < previous {
            return Ok(LineCheck::OutOfOrder {
                first: entry.clone(),
            });
        }
        previous = own;
    }
    Ok(LineCheck::InOrder)
}

fn wave_index(layers: &[Vec<String>], item: &str) -> Option<usize> {
    layers
        .iter()
        .position(|wave| wave.iter().any(|member| member == item))
}
