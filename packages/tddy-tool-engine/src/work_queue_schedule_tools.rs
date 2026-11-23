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
    vec![
        ToolDef {
            name: WORK_ITEM_SCHEDULE.to_string(),
            description: "Arm a work item to merge or to propagate once its conditions hold. A \
                merge waits on order, current, checks and review; a propagation on inbox, conflicts, \
                the base's checks and worktree. `skip_review` lets a merge go once CI is green \
                without an approval, never past a change request."
                .to_string(),
            input_schema_json: r#"{"type":"object","properties":{"item_id":{"type":"string"},"kind":{"type":"string","enum":["merge","propagate"]},"skip_review":{"type":"boolean"}},"required":["item_id","kind"],"additionalProperties":false}"#.to_string(),
        },
        ToolDef {
            name: WORK_QUEUE_SCHEDULE_WAVE.to_string(),
            description: "Arm a merge on every item of a wave that has an open PR, bottom-up."
                .to_string(),
            input_schema_json: r#"{"type":"object","properties":{"wave":{"type":"integer","minimum":1},"skip_review":{"type":"boolean"}},"required":["wave"],"additionalProperties":false}"#.to_string(),
        },
        ToolDef {
            name: WORK_ITEM_UNSCHEDULE.to_string(),
            description: "Disarm a work item's armed merge or propagation.".to_string(),
            input_schema_json: r#"{"type":"object","properties":{"item_id":{"type":"string"}},"required":["item_id"],"additionalProperties":false}"#.to_string(),
        },
        ToolDef {
            name: WORK_QUEUE_SCHEDULES.to_string(),
            description: "List the armed merges and propagations with every condition live as met, \
                waiting or blocked, and what each is held up on."
                .to_string(),
            input_schema_json: r#"{"type":"object","properties":{},"additionalProperties":false}"#
                .to_string(),
        },
    ]
}
