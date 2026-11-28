//! The work-item file tools: what a work item carries, attached and detached on the queue
//! (`#workqueue` work item files). Owner writes and reads under `work_queue_access`; the list is
//! also offered to a session the item addresses — a child may re-read what it was given.
//!
//! An agent attaches a file the **queue-owner host** can already read: a path under the owning
//! session's own artifacts, or one inside the project's main repository. An upload from a browser
//! rides the staged path instead — that arm is the web's, and no agent stages bytes.

use serde_json::{json, Value};

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, TARGET_DAEMON_INSTANCE_ID};

pub const WORK_ITEM_ATTACH_FILE: &str = "work_item_attach_file";
pub const WORK_ITEM_DETACH_FILE: &str = "work_item_detach_file";
pub const WORK_ITEM_LIST_FILES: &str = "work_item_list_files";

pub const WORK_ITEM_FILE_TOOL_NAMES: [&str; 3] = [
    WORK_ITEM_ATTACH_FILE,
    WORK_ITEM_DETACH_FILE,
    WORK_ITEM_LIST_FILES,
];

pub fn work_item_file_tool_defs() -> Vec<ToolDef> {
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
    vec![
        tool_def(
            WORK_ITEM_ATTACH_FILE,
            "Attach a file to a work item: every session later started for the item receives it as a start attachment. The source is a path under the queue owner's own session's artifacts (what this session wrote), or a path inside the project's main repository. At most 20 files per item, each at most the daemon's maximum attachment size; a name the item already carries is refused.",
            schema(
                json!({
                    "basename": { "type": "string", "description": "The name the file carries inside the item — one path segment; a name the item already carries is refused." },
                    "artifact_relative_path": { "type": "string", "description": "A path under the queue owner's own session's artifacts (relative to artifacts/). Name exactly one source." },
                    "repo_file_path": { "type": "string", "description": "A path inside the project's main repository. Name exactly one source." },
                }),
                &["item_id", "basename"],
            ),
        ),
        tool_def(
            WORK_ITEM_DETACH_FILE,
            "Remove one of a work item's files by basename. A session already started for the item keeps its own materialized copy.",
            schema(json!({}), &["item_id", "basename"]),
        ),
        tool_def(
            WORK_ITEM_LIST_FILES,
            "List the files a work item carries, with their sizes.",
            schema(json!({}), &["item_id"]),
        ),
    ]
}
