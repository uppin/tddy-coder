//! The work-item link tools (`#workqueue` 6/14): assign or unassign an existing PR, link or unlink a
//! tddy session. Owner writes under `work_queue_access` (n5); final names.

use crate::catalog::ToolDef;

pub const WORK_ITEM_ASSIGN_PR: &str = "work_item_assign_pr";
pub const WORK_ITEM_UNASSIGN_PR: &str = "work_item_unassign_pr";
pub const WORK_ITEM_LINK_SESSION: &str = "work_item_link_session";
pub const WORK_ITEM_UNLINK_SESSION: &str = "work_item_unlink_session";

/// The four link tools, in the order they are advertised.
pub const WORK_ITEM_LINK_TOOL_NAMES: [&str; 4] = [
    WORK_ITEM_ASSIGN_PR,
    WORK_ITEM_UNASSIGN_PR,
    WORK_ITEM_LINK_SESSION,
    WORK_ITEM_UNLINK_SESSION,
];

/// Their definitions: every one takes `item_id` (and the optional queue); assign takes
/// `pull_number`, link and unlink take `session_id`.
pub fn work_item_link_tool_defs() -> Vec<ToolDef> {
    // TODO(n6 pr-and-session-link): implement
    todo!("TODO(n6): implement work_item_link_tool_defs")
}
