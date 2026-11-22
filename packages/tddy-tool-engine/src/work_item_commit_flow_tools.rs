//! `work_item_inbox` and `work_item_outbox` (`#workqueue` 11/14): an item's inbox with sources and
//! conflict predictions, and its outbox per successor. Final names.

use serde_json::json;

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, TARGET_DAEMON_INSTANCE_ID};

pub const WORK_ITEM_INBOX: &str = "work_item_inbox";
pub const WORK_ITEM_OUTBOX: &str = "work_item_outbox";

/// The two tools, in the order they are advertised.
pub const WORK_ITEM_COMMIT_FLOW_TOOL_NAMES: [&str; 2] = [WORK_ITEM_INBOX, WORK_ITEM_OUTBOX];

/// Both take `item_id` (and the optional queue).
pub fn work_item_commit_flow_tool_defs() -> Vec<ToolDef> {
    let schema = json!({
        "type": "object",
        "properties": {
            TARGET_DAEMON_INSTANCE_ID: {
                "type": "string",
                "description": "The host that holds the queue, as work_queue_list reports it; empty for this host."
            },
            "queue_id": {
                "type": "string",
                "description": "The work queue — the id of the session that owns it. The calling session's own queue when omitted."
            },
            "item_id": {
                "type": "string",
                "description": "The work item (id as work_queue_status lists it)."
            },
        },
        "required": ["item_id"],
    });
    vec![
        tool_def(
            WORK_ITEM_INBOX,
            "Read the commits a work item is missing: everything its line ancestors (and the trunk or an outside base) carry that its branch lacks, each attributed to the nearest ancestor carrying it, with a prediction of whether it would merge cleanly onto the item (and the files it would conflict in). Read from remote tips; changes no local branch, index or worktree.",
            schema.clone(),
        ),
        tool_def(
            WORK_ITEM_OUTBOX,
            "Read the commits a work item owes: for each successor on the line, the commits the item carries that the successor lacks.",
            schema,
        ),
    ]
}
