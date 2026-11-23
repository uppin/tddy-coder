//! `work_queue_waves`' answer and the waves the status shares (`#workqueue` 10/14).

use serde_json::{json, Value};
use tddy_core::changeset::WorkQueue;
use tddy_work_queue::lineage;
use tddy_work_queue::waves;

/// `work_queue_waves`' answer: the waves, the derived line, each item's binding needs and the first
/// item the queue's own order steps back at. A cycle in the stored edges is the error, naming it.
pub(super) fn waves_answer(stack: &WorkQueue) -> Result<Value, String> {
    let derived = queue_waves_of(stack).map_err(|e| e.to_string())?;
    Ok(json!({
        "waves": derived.waves.iter().enumerate().map(|(index, items)| json!({
            "wave": index + 1, "items": items,
        })).collect::<Vec<_>>(),
        "derived_line": derived.derived_line,
        "readiness": derived.readiness.iter().map(|(item, waits_on)| json!({
            "item_id": item, "waits_on": waits_on,
        })).collect::<Vec<_>>(),
        "out_of_order_at": derived.out_of_order_at,
    }))
}

/// [`waves::queue_waves`] over a stored queue: its items in order and its stored dependency edges.
pub(super) fn queue_waves_of(
    stack: &WorkQueue,
) -> Result<waves::QueueWaves, lineage::LineageError> {
    let items: Vec<String> = stack.nodes.iter().map(|n| n.node_id.clone()).collect();
    waves::queue_waves(
        &items,
        &lineage::dependencies_of(stack),
        &waves::merged_items(stack),
    )
}

/// An item's wave (1-based; 0 when the edges close a cycle) and its binding needs.
pub(super) fn wave_and_needs(
    derived: Option<&waves::QueueWaves>,
    item_id: &str,
) -> (usize, Vec<String>) {
    let Some(derived) = derived else {
        return (0, Vec::new());
    };
    let wave = derived
        .waves
        .iter()
        .position(|wave| wave.iter().any(|id| id == item_id))
        .map_or(0, |index| index + 1);
    let needs = derived
        .readiness
        .iter()
        .find(|(id, _)| id == item_id)
        .map(|(_, needs)| needs.clone())
        .unwrap_or_default();
    (wave, needs)
}
