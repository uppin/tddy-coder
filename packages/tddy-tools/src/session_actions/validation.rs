//! Validate JSON action input instances against embedded `input_schema` (JSON Schema).

use anyhow::{bail, Context, Result};
use log::{debug, info};
use serde_json::Value;

/// Validate `instance` against `schema` (Draft 2020-12 / project conventions).
pub fn validate_instance_against_schema(instance: &Value, schema: &Value) -> Result<()> {
    info!(
        target: "tddy_tools::session_actions::validation",
        "validate_instance_against_schema start"
    );
    let validator = jsonschema::options()
        .build(schema)
        .context("failed to compile JSON Schema for validation")?;

    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| {
            let ip = e.instance_path().as_str();
            let sp = e.schema_path().as_str();
            if ip.is_empty() {
                e.to_string()
            } else {
                format!("{ip}: {} (schema path: {sp})", e)
            }
        })
        .collect();

    if errors.is_empty() {
        debug!(
            target: "tddy_tools::session_actions::validation",
            "validate_instance_against_schema ok"
        );
        Ok(())
    } else {
        debug!(
            target: "tddy_tools::session_actions::validation",
            "validate_instance_against_schema failed count={}",
            errors.len()
        );
        bail!("input schema validation failed:\n- {}", errors.join("\n- "));
    }
}
