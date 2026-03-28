//! Contract: `goals.json`, `generated/schema-manifest.json`, and proto files stay aligned.

use std::fs;
use tddy_workflow_recipes::schema_pipeline;

#[test]
fn goals_json_matches_manifest_goal_count_and_names() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let goals_raw = fs::read_to_string(manifest_dir.join("goals.json")).expect("read goals.json");
    let goals_root: serde_json::Value = serde_json::from_str(&goals_raw).expect("parse goals.json");
    let goals_arr = goals_root["goals"].as_array().expect("goals array");
    let mut from_goals: Vec<String> = goals_arr
        .iter()
        .map(|g| g["name"].as_str().expect("name").to_string())
        .collect();

    let man_raw = fs::read_to_string(schema_pipeline::generated_manifest_path()).expect("manifest");
    let man: serde_json::Value = serde_json::from_str(&man_raw).expect("parse manifest");
    let mut from_manifest: Vec<String> = man["goals"]
        .as_array()
        .expect("manifest goals")
        .iter()
        .map(|g| g["name"].as_str().expect("name").to_string())
        .collect();

    from_goals.sort();
    from_manifest.sort();
    assert_eq!(
        from_goals, from_manifest,
        "goals.json and generated/schema-manifest.json must list the same goal names"
    );
    assert_eq!(
        from_goals.len(),
        schema_pipeline::expected_proto_basenames().len(),
        "goals.json and proto_basenames.rs must have same length"
    );
}
