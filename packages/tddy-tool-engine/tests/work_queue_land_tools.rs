//! `work_item_propagate` / `work_queue_land` (`#workqueue` 14/14).

use pretty_assertions::assert_eq;
use tddy_tool_engine::work_queue_land_tools::{
    work_queue_land_tool_defs, WORK_ITEM_PROPAGATE, WORK_QUEUE_LAND,
};

#[test]
fn propagate_takes_an_item_and_land_takes_a_wave_range() {
    // When
    let shapes: Vec<(String, serde_json::Value)> = work_queue_land_tool_defs()
        .into_iter()
        .map(|d| {
            let schema: serde_json::Value = serde_json::from_str(&d.input_schema_json).unwrap();
            (d.name, schema["required"].clone())
        })
        .collect();

    // Then
    assert_eq!(
        shapes,
        vec![
            (
                WORK_ITEM_PROPAGATE.to_string(),
                serde_json::json!(["item_id"])
            ),
            (
                WORK_QUEUE_LAND.to_string(),
                serde_json::json!(["from_wave", "to_wave"])
            ),
        ]
    );
}
