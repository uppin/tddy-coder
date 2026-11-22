//! The work-item annotation tools (`#workqueue` 7/14): labels and keyed badges. Owner writes under
//! `work_queue_access` (n5); final names.

use crate::catalog::ToolDef;

pub const WORK_ITEM_SET_LABELS: &str = "work_item_set_labels";
pub const WORK_ITEM_SET_BADGE: &str = "work_item_set_badge";
pub const WORK_ITEM_CLEAR_BADGE: &str = "work_item_clear_badge";

pub const WORK_ITEM_ANNOTATION_TOOL_NAMES: [&str; 3] = [
    WORK_ITEM_SET_LABELS,
    WORK_ITEM_SET_BADGE,
    WORK_ITEM_CLEAR_BADGE,
];

/// Their definitions: `item_id` always; `labels` (array); `key`, `text`, `tone` (one of `info`,
/// `success`, `warning`, `danger`); `key`.
pub fn work_item_annotation_tool_defs() -> Vec<ToolDef> {
    // TODO(n7 badges-labels): implement
    todo!("TODO(n7): implement work_item_annotation_tool_defs")
}
