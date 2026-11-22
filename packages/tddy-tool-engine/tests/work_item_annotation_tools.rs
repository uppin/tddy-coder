//! The work-item annotation tools (`#workqueue` 7/14).

use pretty_assertions::assert_eq;
use serde_json::Value;
use tddy_tool_engine::work_item_annotation_tools::{
    work_item_annotation_tool_defs, WORK_ITEM_ANNOTATION_TOOL_NAMES, WORK_ITEM_SET_BADGE,
};

#[test]
fn the_annotation_tools_are_defined_in_order() {
    let names: Vec<String> = work_item_annotation_tool_defs()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(
        names,
        WORK_ITEM_ANNOTATION_TOOL_NAMES.map(String::from).to_vec()
    );
}

#[test]
fn a_badge_tone_is_one_of_four() {
    // When
    let def = work_item_annotation_tool_defs()
        .into_iter()
        .find(|d| d.name == WORK_ITEM_SET_BADGE)
        .expect("work_item_set_badge");
    let schema: Value = serde_json::from_str(&def.input_schema_json).expect("a JSON schema");

    // Then
    assert_eq!(
        schema["properties"]["tone"]["enum"],
        serde_json::json!(["info", "success", "warning", "danger"])
    );
}
