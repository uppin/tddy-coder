//! The lineage tools (`#workqueue` 9/14): set an item's dependency edges; read the queue's graph (line
//! edges from PR bases, stored dependency edges, external endpoints). Final names.

use serde_json::{json, Value};

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, TARGET_DAEMON_INSTANCE_ID};

pub const WORK_QUEUE_SET_DEPENDENCIES: &str = "work_queue_set_dependencies";
pub const WORK_QUEUE_GRAPH: &str = "work_queue_graph";

pub const WORK_QUEUE_LINEAGE_TOOL_NAMES: [&str; 2] =
    [WORK_QUEUE_SET_DEPENDENCIES, WORK_QUEUE_GRAPH];

/// `work_queue_set_dependencies { item_id, needs: [item_id] }` and `work_queue_graph {}` (each with the
/// optional queue).
pub fn work_queue_lineage_tool_defs() -> Vec<ToolDef> {
    let schema = |extra: Value, required: &[&str]| {
        let mut properties = json!({
            TARGET_DAEMON_INSTANCE_ID: {
                "type": "string",
                "description": "The host that holds the queue, as work_queue_list reports it; empty for this host."
            },
            "queue_id": {
                "type": "string",
                "description": "The work queue — the id of the session that owns it. The calling session's own queue when omitted."
            },
        });
        for (key, value) in extra.as_object().into_iter().flatten() {
            properties[key] = value.clone();
        }
        json!({ "type": "object", "properties": properties, "required": required })
    };
    vec![
        tool_def(
            WORK_QUEUE_SET_DEPENDENCIES,
            "Replace what a work item needs: the items whose behaviour it builds on (its dependency edges, stored on the queue). Refused, naming the cycle, when the edges would close one; an unknown item is refused. This is not the item's parents (where its branch is cut) and changes nothing on GitHub.",
            schema(
                json!({
                    "item_id": { "type": "string", "description": "The item that needs the others (id as work_queue_status lists it)." },
                    "needs": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "The item ids it needs; empty clears them."
                    },
                }),
                &["item_id", "needs"],
            ),
        ),
        tool_def(
            WORK_QUEUE_GRAPH,
            "Read a work queue's lineage: each item's line edge (what its branch hangs from — an item, the trunk, or a branch outside the queue — read from its pull request's base, else its planned base) and the stored dependency edges.",
            schema(json!({}), &[]),
        ),
    ]
}
