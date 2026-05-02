//! JSON argument validation against an action manifest `input_schema`.

use log::debug;

use serde_json::Value;

use super::error::SessionActionsError;

/// Validate caller JSON (`--data`) against manifest `input_schema` using JSON Schema.
pub fn validate_action_arguments_json(
    input_schema: &Option<Value>,
    args: &Value,
) -> Result<(), SessionActionsError> {
    let Some(schema) = input_schema else {
        return Ok(());
    };

    if schema.as_object().is_none() {
        return Err(SessionActionsError::InvalidSchemaShape(
            "input_schema must be a JSON Schema object".into(),
        ));
    }

    debug!(
        target: "tddy_core::session_actions::validate",
        "validating invoke arguments against input_schema keys={:?}",
        schema
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>())
    );

    let validator = jsonschema::Validator::new(schema).map_err(|e| {
        SessionActionsError::InvalidSchemaShape(format!(
            "could not compile input_schema as JSON Schema: {e}"
        ))
    })?;

    validator.validate(args).map_err(|e| {
        let detail = e.to_string();
        let msg = if detail.to_lowercase().contains("schema") {
            format!("schema validation: {detail}")
        } else {
            format!("schema validation failed: {detail}")
        };
        SessionActionsError::ArgumentsViolateSchema(msg)
    })
}
