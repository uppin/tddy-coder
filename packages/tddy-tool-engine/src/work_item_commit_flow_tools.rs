//! `work_item_inbox` and `work_item_outbox` (`#workqueue` 11/14): an item's inbox with sources and
//! conflict predictions, and its outbox per successor. Final names.

use crate::catalog::ToolDef;

pub const WORK_ITEM_INBOX: &str = "work_item_inbox";
pub const WORK_ITEM_OUTBOX: &str = "work_item_outbox";

/// Both take `item_id` (and the optional queue).
pub fn work_item_commit_flow_tool_defs() -> Vec<ToolDef> {
    // TODO(n11 commit-inbox-outbox): implement
    todo!("TODO(n11): implement work_item_commit_flow_tool_defs")
}
