//! `work_item_propagate` and `work_queue_land` (`#workqueue` 14/14). `work_item_merge` and
//! `work_item_repoint` keep their names and run through the same engines. Final names.

use serde_json::json;

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, TARGET_DAEMON_INSTANCE_ID};

pub const WORK_ITEM_PROPAGATE: &str = "work_item_propagate";
pub const WORK_QUEUE_LAND: &str = "work_queue_land";

/// The two tools, in the order they are advertised.
pub const WORK_QUEUE_LAND_TOOL_NAMES: [&str; 2] = [WORK_ITEM_PROPAGATE, WORK_QUEUE_LAND];

fn target_properties() -> serde_json::Value {
    json!({
        TARGET_DAEMON_INSTANCE_ID: {
            "type": "string",
            "description": "The host that holds the queue, as work_queue_list reports it; empty for this host."
        },
        "queue_id": {
            "type": "string",
            "description": "The work queue — the id of the session that owns it. The calling session's own queue when omitted."
        },
    })
}

/// `work_item_propagate { item_id, force? }`, `work_queue_land { from_wave, to_wave }`.
pub fn work_queue_land_tool_defs() -> Vec<ToolDef> {
    let mut propagate = target_properties();
    propagate["item_id"] = json!({
        "type": "string",
        "description": "The work item to rebase onto its line parent (id as work_queue_status lists it)."
    });
    propagate["force"] = json!({
        "type": "boolean",
        "description": "Rebase although a conflict is predicted. Never overrides a dirty, unpushed, unpulled or diverged local branch."
    });
    let mut land = target_properties();
    land["from_wave"] = json!({
        "type": "integer",
        "description": "The first wave to land (waves as work_queue_status derives them)."
    });
    land["to_wave"] = json!({
        "type": "integer",
        "description": "The last wave to land."
    });
    vec![
        tool_def(
            WORK_ITEM_PROPAGATE,
            "Rebase a work item onto its line parent's tip, in the item's own worktree, and push with lease; retarget its PR when the line parent changed. Refuses, naming the state and the actions that fix it, when the local branch is dirty, unpushed, unpulled or diverged (force does not override that); refuses a predicted conflict unless forced.",
            json!({"type": "object", "properties": propagate, "required": ["item_id"]}),
        ),
        tool_def(
            WORK_QUEUE_LAND,
            "Land a range of waves in derived line order: merge each item's PR, propagate its successors onto the new base, then take the next; stops at the first item whose conditions are not met, naming it.",
            json!({"type": "object", "properties": land, "required": ["from_wave", "to_wave"]}),
        ),
    ]
}
