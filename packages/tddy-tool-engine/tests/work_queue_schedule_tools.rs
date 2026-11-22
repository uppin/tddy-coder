//! The schedule tools (`#workqueue` 13/14).

use pretty_assertions::assert_eq;
use tddy_tool_engine::work_queue_schedule_tools::{
    work_queue_schedule_tool_defs, WORK_ITEM_SCHEDULE, WORK_QUEUE_SCHEDULE_TOOL_NAMES,
};

#[test]
fn the_schedule_tools_are_defined_and_an_item_is_armed_to_merge_or_propagate() {
    // When
    let defs = work_queue_schedule_tool_defs();
    let schedule = defs
        .iter()
        .find(|d| d.name == WORK_ITEM_SCHEDULE)
        .expect("work_item_schedule");
    let schema: serde_json::Value = serde_json::from_str(&schedule.input_schema_json).unwrap();

    // Then
    assert_eq!(
        (
            defs.iter().map(|d| d.name.clone()).collect::<Vec<_>>(),
            schema["properties"]["kind"]["enum"].clone(),
            schema["required"].clone(),
        ),
        (
            WORK_QUEUE_SCHEDULE_TOOL_NAMES.map(String::from).to_vec(),
            serde_json::json!(["merge", "propagate"]),
            serde_json::json!(["item_id", "kind"]),
        )
    );
}
