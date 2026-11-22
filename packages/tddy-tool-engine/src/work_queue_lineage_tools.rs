//! The lineage tools (`#workqueue` 9/14): set an item's dependency edges; read the queue's graph (line
//! edges from PR bases, stored dependency edges, external endpoints). Final names.

use crate::catalog::ToolDef;

pub const WORK_QUEUE_SET_DEPENDENCIES: &str = "work_queue_set_dependencies";
pub const WORK_QUEUE_GRAPH: &str = "work_queue_graph";

pub const WORK_QUEUE_LINEAGE_TOOL_NAMES: [&str; 2] =
    [WORK_QUEUE_SET_DEPENDENCIES, WORK_QUEUE_GRAPH];

/// `work_queue_set_dependencies { item_id, needs: [item_id] }` and `work_queue_graph {}` (each with the
/// optional queue).
pub fn work_queue_lineage_tool_defs() -> Vec<ToolDef> {
    // TODO(n9 lineage-graph): implement
    todo!("TODO(n9): implement work_queue_lineage_tool_defs")
}
