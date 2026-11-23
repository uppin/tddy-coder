//! `work_item_inbox` / `work_item_outbox` (`#workqueue` 11/14).

use pretty_assertions::assert_eq;
use tddy_tool_engine::work_item_commit_flow_tools::{
    work_item_commit_flow_tool_defs, WORK_ITEM_INBOX, WORK_ITEM_OUTBOX,
};

#[test]
fn the_inbox_and_outbox_tools_each_take_an_item() {
    // When
    let defs = work_item_commit_flow_tool_defs();

    // Then
    let shapes: Vec<(String, serde_json::Value)> = defs
        .into_iter()
        .map(|d| {
            let schema: serde_json::Value = serde_json::from_str(&d.input_schema_json).unwrap();
            (d.name, schema["required"].clone())
        })
        .collect();
    assert_eq!(
        shapes,
        vec![
            (WORK_ITEM_INBOX.to_string(), serde_json::json!(["item_id"])),
            (WORK_ITEM_OUTBOX.to_string(), serde_json::json!(["item_id"])),
        ]
    );
}
