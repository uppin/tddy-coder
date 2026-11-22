//! `work_queue_waves` (`#workqueue` 10/14).

use tddy_tool_engine::work_queue_waves_tools::{work_queue_waves_tool_def, WORK_QUEUE_WAVES};

#[test]
fn work_queue_waves_is_defined_and_needs_no_argument() {
    // When
    let def = work_queue_waves_tool_def();
    let schema: serde_json::Value = serde_json::from_str(&def.input_schema_json).expect("schema");

    // Then
    assert_eq!(def.name, WORK_QUEUE_WAVES);
    assert!(
        schema["required"].as_array().is_none_or(Vec::is_empty),
        "{schema}"
    );
}
