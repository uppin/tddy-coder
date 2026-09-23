//! `ListAgentModels`' probe: the `tddy-tools list-models` argv, the parse of its JSON, and the
//! short-lived cache in front of it.
//!
//! Moved from `tddy-session-lifecycle`'s `connection_service.rs` with the catalogue handlers, their
//! only caller.

use tddy_rpc::Status;
use tddy_service::proto::catalog::{ListAgentModelsResponse, ModelInfo as CatalogModelInfo};

/// TTL for the per-(agent, daemon) model-probe cache. A probe spawns a subprocess and may hit the
/// network, so results are cached briefly to avoid re-probing on every agent toggle in the UI.
pub(super) const AGENT_MODELS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(60);

#[allow(clippy::type_complexity)]
static AGENT_MODELS_CACHE: std::sync::OnceLock<
    std::sync::Mutex<
        std::collections::HashMap<String, (std::time::Instant, ListAgentModelsResponse)>,
    >,
> = std::sync::OnceLock::new();

pub(super) fn agent_models_cache() -> &'static std::sync::Mutex<
    std::collections::HashMap<String, (std::time::Instant, ListAgentModelsResponse)>,
> {
    AGENT_MODELS_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Build the `tddy-tools list-models` argv for an agent probe. Always `["list-models", "--agent",
/// <agent>]`; appends `["--cursor-cli-path", <path>]` only when probing `cursor` with a resolved
/// path, so the impersonated child execs the fully-qualified binary instead of a PATH lookup.
pub(super) fn list_models_probe_args(
    agent: &str,
    cursor_cli_path: Option<&std::path::Path>,
) -> Vec<String> {
    let mut args = vec![
        "list-models".to_string(),
        "--agent".to_string(),
        agent.to_string(),
    ];
    if agent == "cursor" {
        if let Some(path) = cursor_cli_path {
            args.push("--cursor-cli-path".to_string());
            args.push(path.to_string_lossy().into_owned());
        }
    }
    args
}

/// Parse the JSON stdout of `tddy-tools list-models --agent <id>`
/// (`{"models":[{"id":..,"label":..}],"default_model":".."}`) into a `ListAgentModelsResponse`.
/// Malformed output is a hard error — a failed probe must not look like an empty catalog.
pub(super) fn parse_agent_models_json(stdout: &str) -> Result<ListAgentModelsResponse, Status> {
    #[derive(serde::Deserialize)]
    struct ModelJson {
        id: String,
        label: String,
    }
    #[derive(serde::Deserialize)]
    struct ModelsJson {
        models: Vec<ModelJson>,
        default_model: String,
    }
    let parsed: ModelsJson = serde_json::from_str(stdout.trim())
        .map_err(|e| Status::internal(format!("failed to parse list-models output: {e}")))?;
    Ok(ListAgentModelsResponse {
        models: parsed
            .models
            .into_iter()
            .map(|m| CatalogModelInfo {
                id: m.id,
                label: m.label,
            })
            .collect(),
        default_model: parsed.default_model,
    })
}

#[cfg(test)]
mod parse_tests;

#[cfg(test)]
mod probe_tests;
