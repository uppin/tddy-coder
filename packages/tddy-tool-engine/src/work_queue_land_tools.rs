//! `work_item_propagate` and `work_queue_land` (`#workqueue` 14/14). `work_item_merge` and
//! `work_item_repoint` keep their names and run through the same engines. Final names.

use crate::catalog::ToolDef;

pub const WORK_ITEM_PROPAGATE: &str = "work_item_propagate";
pub const WORK_QUEUE_LAND: &str = "work_queue_land";

/// `work_item_propagate { item_id, force? }`, `work_queue_land { from_wave, to_wave }`.
pub fn work_queue_land_tool_defs() -> Vec<ToolDef> {
    // TODO(n14 land-propagate): implement
    todo!("TODO(n14): implement work_queue_land_tool_defs")
}
