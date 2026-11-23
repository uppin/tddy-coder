//! The lineage tools (`#workqueue` 9/14).

use pretty_assertions::assert_eq;
use serde_json::Value;
use tddy_tool_engine::work_queue_lineage_tools::{
    work_queue_lineage_tool_defs, WORK_QUEUE_LINEAGE_TOOL_NAMES, WORK_QUEUE_SET_DEPENDENCIES,
};

#[test]
fn the_lineage_tools_are_defined_and_setting_dependencies_takes_an_item_and_its_needs() {
    // When
    let defs = work_queue_lineage_tool_defs();
    let set = defs
        .iter()
        .find(|d| d.name == WORK_QUEUE_SET_DEPENDENCIES)
        .expect("work_queue_set_dependencies");
    let schema: Value = serde_json::from_str(&set.input_schema_json).expect("a JSON schema");

    // Then
    assert_eq!(
        (
            defs.iter().map(|d| d.name.clone()).collect::<Vec<_>>(),
            schema["required"].clone(),
            schema["properties"]["needs"]["type"].clone(),
        ),
        (
            WORK_QUEUE_LINEAGE_TOOL_NAMES.map(String::from).to_vec(),
            serde_json::json!(["item_id", "needs"]),
            serde_json::json!("array"),
        )
    );
}
