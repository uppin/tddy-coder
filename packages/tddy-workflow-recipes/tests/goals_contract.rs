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

const KNOWN_RECIPES: &[&str] = &["tdd"];

#[test]
fn goals_json_includes_merge_pr_report_goal() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let goals_raw = fs::read_to_string(manifest_dir.join("goals.json")).expect("read goals.json");
    let goals_root: serde_json::Value = serde_json::from_str(&goals_raw).expect("parse goals.json");
    let names: Vec<&str> = goals_root["goals"]
        .as_array()
        .expect("goals array")
        .iter()
        .filter_map(|g| g["name"].as_str())
        .collect();
    assert!(
        names.iter().any(|n| *n == "merge-pr-report"),
        "goals.json must register merge-pr-report for structured finalize submit (PRD schema contract); got {:?}",
        names
    );
}

#[test]
fn generated_schemas_are_namespaced_by_recipe() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let generated = manifest_dir.join("generated");

    for recipe in KNOWN_RECIPES {
        let recipe_dir = generated.join(recipe);
        assert!(
            recipe_dir.is_dir(),
            "generated/{recipe}/ directory must exist — schemas should be namespaced under their recipe"
        );

        let goals_raw =
            fs::read_to_string(manifest_dir.join("goals.json")).expect("read goals.json");
        let goals_root: serde_json::Value =
            serde_json::from_str(&goals_raw).expect("parse goals.json");
        let goals_arr = goals_root["goals"].as_array().expect("goals array");
        for g in goals_arr {
            let schema = g["schema"].as_str().expect("schema field");
            let namespaced = recipe_dir.join(schema);
            assert!(
                namespaced.is_file(),
                "generated/{recipe}/{schema} must exist — schema files belong under the recipe namespace, \
                 not at generated/ root"
            );
        }
    }
}

#[test]
fn generated_root_has_no_flat_schema_files() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let generated = manifest_dir.join("generated");

    let flat_schemas: Vec<String> = fs::read_dir(&generated)
        .expect("read generated/")
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".schema.json") && e.path().is_file()
        })
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(
        flat_schemas.is_empty(),
        "generated/ root must not contain flat schema files — found {:?}; \
         schemas should be under generated/{{recipe}}/ subdirectories",
        flat_schemas
    );
}
