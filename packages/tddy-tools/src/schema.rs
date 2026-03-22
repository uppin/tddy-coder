//! JSON Schema validation for structured agent output.
//!
//! Schemas are embedded in the binary via `include_dir`. All schema interaction
//! is via tddy-tools; no schema files are written to disk by tddy-core.

use include_dir::{include_dir, Dir};
use jsonschema::Resource;
use serde_json::Value;
use std::path::Path;

static SCHEMAS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/schemas");

/// Goal name (from JSON "goal" field) to schema file mapping.
const GOAL_SCHEMA_FILES: &[(&str, &str)] = &[
    ("plan", "plan.schema.json"),
    ("acceptance-tests", "acceptance-tests.schema.json"),
    ("red", "red.schema.json"),
    ("green", "green.schema.json"),
    ("evaluate-changes", "evaluate.schema.json"),
    ("validate", "validate-subagents.schema.json"),
    ("refactor", "refactor.schema.json"),
    ("update-docs", "update-docs.schema.json"),
    ("demo", "demo.schema.json"),
];

/// Common schema files (in common/ subdir) with their $id URIs.
const COMMON_SCHEMAS: &[(&str, &str)] = &[
    ("urn:tddy:common/test-info", "common/test-info.schema.json"),
    (
        "urn:tddy:common/skeleton-info",
        "common/skeleton-info.schema.json",
    ),
    (
        "urn:tddy:common/build-result",
        "common/build-result.schema.json",
    ),
    ("urn:tddy:common/issue", "common/issue.schema.json"),
    (
        "urn:tddy:common/changeset-sync",
        "common/changeset-sync.schema.json",
    ),
    (
        "urn:tddy:common/file-analyzed",
        "common/file-analyzed.schema.json",
    ),
    (
        "urn:tddy:common/test-impact",
        "common/test-impact.schema.json",
    ),
];

/// A single validation error with instance path and message.
#[derive(Debug, Clone)]
pub struct SchemaError {
    pub instance_path: String,
    pub schema_path: String,
    pub message: String,
}

/// Returns the raw JSON Schema string for a goal, or None if not found.
pub fn get_schema(goal: &str) -> Option<&'static str> {
    let (_, filename) = GOAL_SCHEMA_FILES.iter().find(|(g, _)| *g == goal)?;
    let file = SCHEMAS_DIR.get_file(filename)?;
    file.contents_utf8()
}

/// Returns (uri, parsed Value) for all common schemas.
fn get_all_common_schemas() -> Vec<(&'static str, Value)> {
    let mut out = Vec::with_capacity(COMMON_SCHEMAS.len());
    for (uri, path) in COMMON_SCHEMAS {
        if let Some(file) = SCHEMAS_DIR.get_file(path) {
            if let Some(s) = file.contents_utf8() {
                if let Ok(v) = serde_json::from_str(s) {
                    out.push((*uri, v));
                }
            }
        }
    }
    out
}

/// Validates JSON string against the goal's schema. Returns Ok(()) if valid, Err with error list if invalid.
pub fn validate_output(goal: &str, json_str: &str) -> Result<(), Vec<SchemaError>> {
    let schema_str = get_schema(goal).ok_or_else(|| {
        vec![SchemaError {
            instance_path: String::new(),
            schema_path: String::new(),
            message: format!("schema not found for goal: {}", goal),
        }]
    })?;

    let instance: Value = serde_json::from_str(json_str).map_err(|e| {
        vec![SchemaError {
            instance_path: String::new(),
            schema_path: String::new(),
            message: format!("invalid JSON: {}", e),
        }]
    })?;

    let schema: Value = serde_json::from_str(schema_str).map_err(|e| {
        vec![SchemaError {
            instance_path: String::new(),
            schema_path: String::new(),
            message: format!("invalid schema: {}", e),
        }]
    })?;

    let mut opts = jsonschema::options();
    for (uri, common_schema) in get_all_common_schemas() {
        opts = opts.with_resource(uri, Resource::from_contents(common_schema));
    }

    let validator = opts.build(&schema).map_err(|e| {
        vec![SchemaError {
            instance_path: String::new(),
            schema_path: String::new(),
            message: format!("failed to build validator: {}", e),
        }]
    })?;

    let errors: Vec<SchemaError> = validator
        .iter_errors(&instance)
        .map(|err| SchemaError {
            instance_path: err.instance_path().as_str().to_string(),
            schema_path: err.schema_path().as_str().to_string(),
            message: err.to_string(),
        })
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Formats validation errors for inclusion in retry prompts.
pub fn format_validation_errors(errors: &[SchemaError]) -> String {
    let mut out = String::new();
    for e in errors {
        if out.is_empty() {
            out.push_str("- ");
        } else {
            out.push_str("\n- ");
        }
        if e.instance_path.is_empty() {
            out.push_str(&e.message);
        } else {
            out.push_str(&format!("{}: {}", e.instance_path, e.message));
        }
    }
    out
}

/// Writes the goal schema and all common schemas to the given path.
/// Used by `get-schema` subcommand when -o is specified.
pub fn write_schema_to_path(goal: &str, out_path: &Path) -> std::io::Result<()> {
    let schema_str = get_schema(goal).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown goal: {}", goal),
        )
    })?;

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(out_path, schema_str)?;

    let base_dir = out_path.parent().unwrap();
    for (_uri, path) in COMMON_SCHEMAS {
        if let Some(file) = SCHEMAS_DIR.get_file(path) {
            if let Some(contents) = file.contents_utf8() {
                let common_out = base_dir.join(path);
                if let Some(parent) = common_out.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&common_out, contents)?;
            }
        }
    }

    Ok(())
}

/// Tip for validation errors.
pub fn validation_error_tip(goal: &str) -> String {
    format!(
        "Tip: Run `tddy-tools get-schema {}` to see the expected output format.",
        goal
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_schema_returns_content_for_all_goals() {
        for (goal, _) in GOAL_SCHEMA_FILES {
            let content =
                get_schema(goal).unwrap_or_else(|| panic!("schema for {} should exist", goal));
            assert!(!content.is_empty());
            assert!(content.contains("$schema"));
        }
    }

    #[test]
    fn get_schema_returns_none_for_unknown_goal() {
        assert!(get_schema("unknown").is_none());
    }
}
