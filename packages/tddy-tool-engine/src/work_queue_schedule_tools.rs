//! The schedule tools (`#workqueue` 13/14): arm a merge or a propagation on an item, arm a wave's
//! merges, disarm, list with live conditions. Final names.

use crate::catalog::ToolDef;

pub const WORK_ITEM_SCHEDULE: &str = "work_item_schedule";
pub const WORK_QUEUE_SCHEDULE_WAVE: &str = "work_queue_schedule_wave";
pub const WORK_ITEM_UNSCHEDULE: &str = "work_item_unschedule";
pub const WORK_QUEUE_SCHEDULES: &str = "work_queue_schedules";

pub const WORK_QUEUE_SCHEDULE_TOOL_NAMES: [&str; 4] = [
    WORK_ITEM_SCHEDULE,
    WORK_QUEUE_SCHEDULE_WAVE,
    WORK_ITEM_UNSCHEDULE,
    WORK_QUEUE_SCHEDULES,
];

/// `work_item_schedule { item_id, kind: merge|propagate, skip_review? }`,
/// `work_queue_schedule_wave { wave, skip_review? }`, `work_item_unschedule { item_id }`,
/// `work_queue_schedules {}`.
pub fn work_queue_schedule_tool_defs() -> Vec<ToolDef> {
    // TODO(n13 schedules): implement
    todo!("TODO(n13): implement work_queue_schedule_tool_defs")
}
