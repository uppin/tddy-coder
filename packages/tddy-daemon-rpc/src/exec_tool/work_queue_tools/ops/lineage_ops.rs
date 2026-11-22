//! The lineage tools' answers (`#workqueue` 9/14): `work_queue_set_dependencies` writes an item's
//! stored dependency edges; `work_queue_graph`'s JSON is shaped here.

use serde_json::{json, Value};
use tddy_work_queue::item_ops::WorkQueueHome;
use tddy_work_queue::lineage;

use super::OpEnv;

/// `work_queue_set_dependencies`: replace what `item_id` needs under the record lock; a refused edit
/// (unknown item, a cycle) writes nothing and is the error.
pub(super) fn set_dependencies(
    env: &OpEnv,
    item_id: &str,
    needs: Vec<String>,
) -> Result<Value, String> {
    let mut refused = None;
    env.home()?
        .update_stack(|queue| {
            let mut edited = queue.clone();
            match lineage::set_dependencies(&mut edited, item_id, needs) {
                Ok(()) => *queue = edited,
                Err(error) => refused = Some(error),
            }
        })
        .map_err(|e| format!("set dependencies: {e}"))?;
    if let Some(error) = refused {
        return Err(error.to_string());
    }
    Ok(json!({
        "item_id": item_id,
        "dependencies": dependencies_json(&lineage::dependencies_of(&env.stack()?)),
    }))
}

fn dependencies_json(edges: &[lineage::DependencyEdge]) -> Value {
    Value::Array(
        edges
            .iter()
            .map(|edge| json!({ "from": edge.from, "to": edge.to }))
            .collect(),
    )
}

/// `work_queue_graph`'s answer: each item's line edge and the stored dependency edges.
pub(in super::super) fn graph_json(
    graph: &lineage::QueueGraph,
    unavailable_reason: Option<&str>,
) -> Value {
    let mut answer = json!({
        "line_edges": graph.line.iter().map(|(item_id, endpoint)| json!({
            "item_id": item_id, "kind": endpoint.kind(), "endpoint": endpoint.name(),
        })).collect::<Vec<_>>(),
        "dependencies": dependencies_json(&graph.dependencies),
    });
    if let Some(reason) = unavailable_reason {
        answer["unavailable_reason"] = json!(reason);
    }
    answer
}
