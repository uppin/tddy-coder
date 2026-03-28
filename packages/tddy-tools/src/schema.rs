//! JSON Schema validation for structured agent output.
//!
//! Goal ↔ schema file mapping is generated from `tddy-workflow-recipes/goals.json` (`build.rs` → `OUT_DIR/goal_registry.rs`).
//! Embedded files come from `tddy-workflow-recipes/generated/` (see that crate's `build.rs`).
//! All schema interaction is via tddy-tools; no schema files are written to disk by tddy-core.

use include_dir::{include_dir, Dir};
use jsonschema::Resource;
use log::{debug, error, info};
use serde_json::Value;
use std::path::Path;
use std::sync::OnceLock;

static SCHEMAS_DIR: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../tddy-workflow-recipes/generated");

include!(concat!(env!("OUT_DIR"), "/goal_registry.rs"));

/// Common schema files (under `tdd/common/` subdir) with their `$id` URIs.
const COMMON_SCHEMAS: &[(&str, &str)] = &[
    (
        "urn:tddy:common/test-info",
        "tdd/common/test-info.schema.json",
    ),
    (
        "urn:tddy:common/skeleton-info",
        "tdd/common/skeleton-info.schema.json",
    ),
    (
        "urn:tddy:common/build-result",
        "tdd/common/build-result.schema.json",
    ),
    ("urn:tddy:common/issue", "tdd/common/issue.schema.json"),
    (
        "urn:tddy:common/changeset-sync",
        "tdd/common/changeset-sync.schema.json",
    ),
    (
        "urn:tddy:common/file-analyzed",
        "tdd/common/file-analyzed.schema.json",
    ),
    (
        "urn:tddy:common/test-impact",
        "tdd/common/test-impact.schema.json",
    ),
];

static COMMON_SCHEMAS_PARSED: OnceLock<Result<Vec<(&'static str, Value)>, String>> =
    OnceLock::new();

/// A single validation error with instance path and message.
#[derive(Debug, Clone)]
pub struct SchemaError {
    pub instance_path: String,
    pub schema_path: String,
    pub message: String,
}

/// CLI goal names from `goals.json` (same order as registry).
pub fn goal_cli_names() -> Vec<&'static str> {
    GOAL_SCHEMA_FILES.iter().map(|(g, _)| *g).collect()
}

fn load_common_schemas() -> Result<Vec<(&'static str, Value)>, String> {
    let mut out = Vec::with_capacity(COMMON_SCHEMAS.len());
    for (uri, path) in COMMON_SCHEMAS {
        let file = SCHEMAS_DIR
            .get_file(path)
            .ok_or_else(|| format!("missing embedded common schema file: {}", path))?;
        let s = file
            .contents_utf8()
            .ok_or_else(|| format!("common schema not utf-8: {}", path))?;
        let v: Value = serde_json::from_str(s)
            .map_err(|e| format!("invalid JSON in embedded common schema {}: {}", path, e))?;
        debug!(
            target: "tddy_tools::schema",
            "loaded common schema uri={} path={}",
            uri,
            path
        );
        out.push((*uri, v));
    }
    Ok(out)
}

fn common_schemas_or_err() -> Result<&'static Vec<(&'static str, Value)>, String> {
    let r = COMMON_SCHEMAS_PARSED.get_or_init(load_common_schemas);
    match r {
        Ok(v) => Ok(v),
        Err(e) => Err(e.clone()),
    }
}

/// Returns the raw JSON Schema string for a goal, or None if not found.
pub fn get_schema(goal: &str) -> Option<&'static str> {
    let (_, filename) = GOAL_SCHEMA_FILES.iter().find(|(g, _)| *g == goal)?;
    debug!(
        target: "tddy_tools::schema",
        "resolve goal schema goal={} file={}",
        goal,
        filename
    );
    let file = SCHEMAS_DIR.get_file(filename)?;
    file.contents_utf8()
}

/// Validates JSON string against the goal's schema. Returns Ok(()) if valid, Err with error list if invalid.
pub fn validate_output(goal: &str, json_str: &str) -> Result<(), Vec<SchemaError>> {
    info!(
        target: "tddy_tools::schema",
        "validate_output start goal={} bytes={}",
        goal,
        json_str.len()
    );

    let common_list = match common_schemas_or_err() {
        Ok(v) => v,
        Err(msg) => {
            error!(target: "tddy_tools::schema", "{}", msg);
            return Err(vec![SchemaError {
                instance_path: String::new(),
                schema_path: String::new(),
                message: msg,
            }]);
        }
    };

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
    for (uri, common_schema) in common_list.iter() {
        opts = opts.with_resource(*uri, Resource::from_contents(common_schema.clone()));
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
        info!(target: "tddy_tools::schema", "validate_output ok goal={}", goal);
        Ok(())
    } else {
        debug!(
            target: "tddy_tools::schema",
            "validate_output errors goal={} count={}",
            goal,
            errors.len()
        );
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
        "Tip: Run `tddy-tools get-schema {}` for the expected JSON shape; run `tddy-tools list-schemas` to list all workflow goals.",
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

    #[test]
    fn common_schemas_load_successfully() {
        common_schemas_or_err().expect("all embedded common schemas must parse");
    }
}
