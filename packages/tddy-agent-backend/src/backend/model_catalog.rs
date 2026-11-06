//! The `{"models":[…],"default_model":".."}` JSON contract a model listing is rendered as.
//!
//! The daemon shells out to `tddy-tools list-models --agent <id>` for `ListAgentModels` and
//! parses this JSON. Which models an agent offers is its spec's `coding_agent.models` (read by
//! `tddy-tools`, which can load the registry); this module owns only the rendering. See
//! docs/ft/web/tool-session-model-selection.md.

use serde::Serialize;

use super::{BackendModel, BackendModels};

/// JSON wire shape for a single model in the daemon⇄tools contract.
#[derive(Serialize)]
struct ModelJson {
    id: String,
    label: String,
}

/// JSON wire shape for a backend's model catalog.
#[derive(Serialize)]
struct ModelsJson {
    models: Vec<ModelJson>,
    default_model: String,
}

impl From<&BackendModels> for ModelsJson {
    fn from(catalog: &BackendModels) -> Self {
        Self {
            models: catalog
                .models
                .iter()
                .map(|m: &BackendModel| ModelJson {
                    id: m.id.clone(),
                    label: m.label.clone(),
                })
                .collect(),
            default_model: catalog.default_model.clone(),
        }
    }
}

/// Render a [`BackendModels`] catalog as the daemon⇄tools JSON contract:
/// `{"models":[{"id":..,"label":..}],"default_model":".."}`.
#[must_use]
pub fn render_models_json(models: &BackendModels) -> String {
    serde_json::to_string(&ModelsJson::from(models))
        .expect("BackendModels serializes to JSON infallibly")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendModel;

    #[test]
    fn renders_backend_models_as_the_daemon_tools_json_contract() {
        // Given
        let catalog = BackendModels {
            models: vec![
                BackendModel::new("opus", "Claude Opus"),
                BackendModel::new("sonnet", "Claude Sonnet"),
            ],
            default_model: "opus".to_string(),
        };

        // When
        let json = render_models_json(&catalog);

        // Then — a well-formed contract the daemon can parse back
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(parsed["default_model"], "opus");
        assert_eq!(parsed["models"][0]["id"], "opus");
        assert_eq!(parsed["models"][0]["label"], "Claude Opus");
        assert_eq!(parsed["models"][1]["id"], "sonnet");
    }
}
