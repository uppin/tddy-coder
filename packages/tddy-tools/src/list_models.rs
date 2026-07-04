//! `tddy-tools list-models --agent <id>` — enumerate a backend's selectable models as JSON.
//!
//! The daemon shells out to this subcommand for `ListAgentModels` and parses the JSON. This module
//! owns the JSON contract (rendering side); the daemon owns the parsing side. See
//! docs/ft/web/tool-session-model-selection.md.

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use serde::Serialize;
use tddy_core::backend::{
    claude_cli_models, AnyBackend, ClaudeAcpBackend, ClaudeCodeBackend, CodexAcpBackend,
    CodexBackend, CursorBackend, StubBackend,
};
use tddy_core::backend::{BackendModels, CodingBackend};

/// `tddy-tools list-models --agent <id>` — print an agent's selectable models as JSON.
#[derive(Parser)]
#[command(name = "list-models")]
pub struct ListModelsArgs {
    /// Agent id ("claude", "claude-acp", "cursor", "codex", "codex-acp", "stub") or the
    /// pseudo-agent "claude-cli".
    #[arg(long)]
    pub agent: String,

    /// Override the cursor (`agent`) binary path.
    #[arg(long)]
    pub cursor_cli_path: Option<PathBuf>,

    /// Override the `claude-agent-acp` binary path.
    #[arg(long)]
    pub claude_acp_cli_path: Option<PathBuf>,

    /// Override the `codex-acp` binary path.
    #[arg(long)]
    pub codex_acp_cli_path: Option<PathBuf>,
}

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
                .map(|m| ModelJson {
                    id: m.id.clone(),
                    label: m.label.clone(),
                })
                .collect(),
            default_model: catalog.default_model.clone(),
        }
    }
}

/// The `claude-cli` session type is not a `CodingBackend`; it enumerates a curated catalog.
pub const CLAUDE_CLI_AGENT: &str = "claude-cli";

/// Render a [`BackendModels`] catalog as the daemon⇄tools JSON contract:
/// `{"models":[{"id":..,"label":..}],"default_model":".."}`.
#[must_use]
pub fn render_models_json(models: &BackendModels) -> String {
    serde_json::to_string(&ModelsJson::from(models))
        .expect("BackendModels serializes to JSON infallibly")
}

/// Enumerate the models an agent supports and render them as the JSON contract.
///
/// `claude-cli` resolves to the curated Claude catalog directly; every other agent is enumerated
/// through its [`tddy_core::backend::CodingBackend::list_models`] (querying the underlying command
/// for cursor/ACP backends, or the curated list otherwise).
pub async fn run_list_models(args: &ListModelsArgs) -> anyhow::Result<()> {
    let catalog = resolve_models(args).await?;
    println!("{}", render_models_json(&catalog));
    Ok(())
}

async fn resolve_models(args: &ListModelsArgs) -> anyhow::Result<BackendModels> {
    let agent = args.agent.trim();
    if agent == CLAUDE_CLI_AGENT {
        return Ok(claude_cli_models());
    }
    let backend =
        build_backend(args).with_context(|| format!("no backend for agent {:?}", agent))?;
    backend
        .list_models()
        .await
        .with_context(|| format!("listing models for agent {:?}", agent))
}

/// Construct the backend for `agent`, honoring any CLI-path overrides. Errors on an unknown agent.
fn build_backend(args: &ListModelsArgs) -> anyhow::Result<AnyBackend> {
    let backend = match args.agent.trim() {
        "claude" => AnyBackend::Claude(ClaudeCodeBackend::new()),
        "claude-acp" => AnyBackend::ClaudeAcp(match &args.claude_acp_cli_path {
            Some(p) => ClaudeAcpBackend::with_agent_path(p.clone()),
            None => ClaudeAcpBackend::new(),
        }),
        "cursor" => AnyBackend::Cursor(match &args.cursor_cli_path {
            Some(p) => CursorBackend::with_path(p.clone()),
            None => CursorBackend::new(),
        }),
        "codex" => AnyBackend::Codex(CodexBackend::new()),
        "codex-acp" => AnyBackend::CodexAcp(match &args.codex_acp_cli_path {
            Some(p) => CodexAcpBackend::with_agent_path(p.clone()),
            None => CodexAcpBackend::new(),
        }),
        "stub" => AnyBackend::Stub(StubBackend::new()),
        other => anyhow::bail!("unknown agent {:?}", other),
    };
    Ok(backend)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::backend::BackendModel;

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
