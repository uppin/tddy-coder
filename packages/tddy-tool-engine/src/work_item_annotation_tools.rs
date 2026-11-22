//! The work-item annotation tools (`#workqueue` 7/14): labels and keyed badges. Owner writes under
//! `work_queue_access` (n5); final names.

use serde_json::{json, Value};

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, TARGET_DAEMON_INSTANCE_ID};

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
    let schema = |extra: Value, required: &[&str]| {
        let mut properties = json!({
            "item_id": { "type": "string", "description": "The work item (id as work_queue_status lists it)." },
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
    let key = json!({ "type": "string", "description": "The badge's key, unique on the item." });
    vec![
        tool_def(
            WORK_ITEM_SET_LABELS,
            "Replace a work item's labels with the given set (deduplicated). The queue view filters by label. At most 16 labels of 40 characters each; more is refused.",
            schema(
                json!({ "labels": { "type": "array", "items": { "type": "string" }, "description": "The labels; empty clears them." } }),
                &["item_id", "labels"],
            ),
        ),
        tool_def(
            WORK_ITEM_SET_BADGE,
            "Set a badge on a work item — a short text in a tone. Setting a key again replaces that badge. At most 8 badges of 60 characters of text; more is refused.",
            schema(
                json!({
                    "key": key.clone(),
                    "text": { "type": "string", "description": "The badge's short text." },
                    "tone": { "type": "string", "enum": ["info", "success", "warning", "danger"], "description": "How the badge is shown." },
                }),
                &["item_id", "key", "text", "tone"],
            ),
        ),
        tool_def(
            WORK_ITEM_CLEAR_BADGE,
            "Remove a work item's badge by key; a missing key changes nothing.",
            schema(json!({ "key": key }), &["item_id", "key"]),
        ),
    ]
}
