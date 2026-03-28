//! Workflow schema **manifest**: registered goal names from `schema-manifest.json` (generated from `goals.json`).
//! For JSON Schema **validation** and embedded files, see [`crate::schema`].

use log::{debug, info};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Embedded manifest produced by `tddy-workflow-recipes/build.rs` (from `goals.json`).
const SCHEMA_MANIFEST_JSON: &str =
    include_str!("../../tddy-workflow-recipes/generated/schema-manifest.json");

/// Path to the generated manifest (`schema-manifest.json`) produced by the workflow-recipes build.
pub fn schema_manifest_path() -> PathBuf {
    debug!(target: "tddy_tools::schema_manifest", "schema_manifest_path");
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tddy-workflow-recipes/generated/schema-manifest.json")
}

#[derive(Debug)]
pub enum SchemaManifestError {
    Parse(String),
}

impl std::fmt::Display for SchemaManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaManifestError::Parse(s) => write!(f, "schema manifest: {}", s),
        }
    }
}

impl std::error::Error for SchemaManifestError {}

#[derive(Debug, Deserialize)]
struct SchemaManifest {
    #[allow(dead_code)]
    version: u32,
    goals: Vec<GoalEntry>,
}

#[derive(Debug, Deserialize)]
struct GoalEntry {
    name: String,
    #[allow(dead_code)]
    schema: String,
    #[allow(dead_code)]
    proto: String,
}

/// Goal names registered for CLI / validation (from generated manifest).
pub fn list_registered_goals() -> Result<Vec<String>, SchemaManifestError> {
    debug!(target: "tddy_tools::schema_manifest", "list_registered_goals");
    let m: SchemaManifest = serde_json::from_str(SCHEMA_MANIFEST_JSON)
        .map_err(|e| SchemaManifestError::Parse(format!("embedded schema-manifest.json: {}", e)))?;
    let names: Vec<String> = m.goals.into_iter().map(|g| g.name).collect();
    info!(
        target: "tddy_tools::schema_manifest",
        "registered workflow goals count={}",
        names.len()
    );
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    #[test]
    fn generated_schema_manifest_exists_next_to_workflow_recipes() {
        assert!(
            schema_manifest_path().is_file(),
            "expected generated schema-manifest.json (proto → JSON Schema pipeline; PRD F2)"
        );
    }

    #[test]
    fn manifest_goal_names_match_goal_registry() {
        let mut from_manifest: Vec<String> = list_registered_goals().expect("manifest must parse");
        let mut from_registry: Vec<String> = schema::goal_cli_names()
            .into_iter()
            .map(String::from)
            .collect();
        from_manifest.sort();
        from_registry.sort();
        assert_eq!(
            from_manifest, from_registry,
            "goals.json → manifest and generated goal_registry.rs must list the same CLI names"
        );
    }
}
