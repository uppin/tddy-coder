//! JSON argument validation against an action manifest `input_schema`.

use log::debug;

use serde_json::Value;

use super::error::SessionActionsError;
use super::manifest::ActionManifest;

/// Field checks for an *authored* manifest before it is established as an action file: a
/// non-empty argv, an id safe to use as a filename under `<session_dir>/actions/` (letters,
/// digits, `-`, `_` only — no path separators or dots), and a compilable `input_schema`. Shared
/// by the in-jail `request_action` retry loop (tddy-tools) and the authoritative host-side
/// `EstablishAction` handler (tddy-sandbox-app) so the two never drift.
pub fn validate_authored_manifest(manifest: &ActionManifest) -> Result<(), SessionActionsError> {
    if manifest.command.is_empty() || manifest.command[0].trim().is_empty() {
        return Err(SessionActionsError::EmptyCommand);
    }
    if manifest.id.trim().is_empty()
        || !manifest
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(SessionActionsError::PathTraversalAttempt {
            purpose: "action id",
            reason: format!(
                "id must contain only letters, digits, `-`, `_` (got {:?})",
                manifest.id
            ),
        });
    }
    if let Some(schema) = &manifest.input_schema {
        if schema.as_object().is_none() {
            return Err(SessionActionsError::InvalidSchemaShape(
                "input_schema must be a JSON Schema object".into(),
            ));
        }
        jsonschema::Validator::new(schema).map_err(|e| {
            SessionActionsError::InvalidSchemaShape(format!(
                "input_schema does not compile as JSON Schema: {e}"
            ))
        })?;
    }
    Ok(())
}

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
