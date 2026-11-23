//! The work-item link tools (`#workqueue` 6/14): assign or unassign an existing PR, link or unlink a
//! tddy session. Owner writes under `work_queue_access` (n5); final names.

use serde_json::{json, Value};

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, TARGET_DAEMON_INSTANCE_ID};

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
    let item_id = json!({ "type": "string", "description": "The work item (id as work_queue_status lists it)." });
    let session_id = json!({ "type": "string", "description": "The tddy session." });
    let schema = |extra: Value, required: &[&str]| {
        let mut properties = json!({
            "item_id": item_id.clone(),
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
            WORK_ITEM_ASSIGN_PR,
            "Assign an existing pull request to a work item. The PR's head branch becomes the item's branch; an item that already records a different branch refuses it. Nothing on GitHub changes.",
            schema(
                json!({ "pull_number": { "type": "integer", "minimum": 1, "description": "The pull request number." } }),
                &["item_id", "pull_number"],
            ),
        ),
        tool_def(
            WORK_ITEM_UNASSIGN_PR,
            "Remove a work item's assigned pull request. The item keeps its branch.",
            schema(json!({}), &["item_id"]),
        ),
        tool_def(
            WORK_ITEM_LINK_SESSION,
            "Link a tddy session to a work item. A linked session may read the queue. Linking it again changes nothing.",
            schema(json!({ "session_id": session_id.clone() }), &["item_id", "session_id"]),
        ),
        tool_def(
            WORK_ITEM_UNLINK_SESSION,
            "Unlink a tddy session from a work item; refused when it is not linked.",
            schema(json!({ "session_id": session_id }), &["item_id", "session_id"]),
        ),
    ]
}
