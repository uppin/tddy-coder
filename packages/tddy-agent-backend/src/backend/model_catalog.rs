//! Model-catalogue assembly and the `{"models":[…],"default_model":".."}` JSON contract.
//!
//! The daemon shells out to `tddy-tools list-models --agent <id>` for `ListAgentModels` and parses
//! this JSON. This module owns the rendering side and the agent-id → backend resolution behind it;
//! the daemon owns the parsing side, and `tddy_tools::list_models` owns the argument parsing and
//! the line printed to stdout. See docs/ft/web/tool-session-model-selection.md.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Serialize;

use super::{
    AnyBackend, BackendModels, ClaudeAcpBackend, ClaudeCodeBackend, CodexAcpBackend, CodexBackend,
    CodingBackend, CursorBackend, OpenCodeAcpBackend, StubBackend,
};
use crate::cli_harness::{cli_harness, models_from_listing, CliHarness, CliModelSource};

/// The `claude-cli` session type is not a `CodingBackend`; it enumerates a curated catalog.
pub const CLAUDE_CLI_AGENT: &str = "claude-cli";

/// The `cursor-cli` session type pseudo-agent for model listing.
pub const CURSOR_CLI_AGENT: &str = "cursor-cli";

/// Overrides for the agent binaries that are enumerated by running them. An absent path means
/// "use the backend's own default".
#[derive(Debug, Default, Clone)]
pub struct BackendCliPaths {
    /// Override the cursor (`agent`) binary path.
    pub cursor_cli_path: Option<PathBuf>,
    /// Override the `opencode` binary path. Names both the `opencode-cli` harness's binary (listed
    /// with `opencode models`) and the binary the `opencode` ACP agent runs (as `opencode acp`).
    pub opencode_cli_path: Option<PathBuf>,
    /// Override the `claude-agent-acp` binary path.
    pub claude_acp_cli_path: Option<PathBuf>,
    /// Override the `codex-acp` binary path.
    pub codex_acp_cli_path: Option<PathBuf>,
}

impl BackendCliPaths {
    /// The override for the binary a CLI harness lists its models with, when one was given.
    fn listing_binary_override(&self, session_type: &str) -> Option<&Path> {
        match session_type {
            "opencode-cli" => self.opencode_cli_path.as_deref(),
            _ => None,
        }
    }
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

/// Render a [`BackendModels`] catalog as the daemon⇄tools JSON contract:
/// `{"models":[{"id":..,"label":..}],"default_model":".."}`.
#[must_use]
pub fn render_models_json(models: &BackendModels) -> String {
    serde_json::to_string(&ModelsJson::from(models))
        .expect("BackendModels serializes to JSON infallibly")
}

/// Enumerate the models `agent` supports.
///
/// A CLI harness session type (`claude-cli`, `cursor-cli`, `opencode-cli`) takes its catalogue from
/// its registry descriptor — curated, or read from the harness's own binary. Every other agent is
/// enumerated through its [`CodingBackend::list_models`] (querying the underlying command for
/// cursor/ACP backends, or the curated list otherwise).
pub async fn resolve_agent_models(
    agent: &str,
    paths: &BackendCliPaths,
) -> anyhow::Result<BackendModels> {
    let agent = agent.trim();
    if let Some(harness) = cli_harness(agent) {
        return match harness.models {
            CliModelSource::Curated(catalogue) => Ok(catalogue()),
            CliModelSource::ListedByBinary(args) => list_by_binary(harness, args, paths).await,
        };
    }
    let backend =
        build_backend(agent, paths).with_context(|| format!("no backend for agent {:?}", agent))?;
    backend
        .list_models()
        .await
        .with_context(|| format!("listing models for agent {:?}", agent))
}

/// The catalogue `harness`'s own binary prints when run with `args`. A binary that cannot be run or
/// exits non-zero is an error — an empty catalogue would read as "no models" rather than "could not
/// ask".
async fn list_by_binary(
    harness: &CliHarness,
    args: &[&str],
    paths: &BackendCliPaths,
) -> anyhow::Result<BackendModels> {
    let binary = paths
        .listing_binary_override(harness.session_type)
        .unwrap_or_else(|| Path::new(harness.default_binary));
    let output = tokio::process::Command::new(binary)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .with_context(|| format!("running {} {}", binary.display(), args.join(" ")))?;
    anyhow::ensure!(
        output.status.success(),
        "{} {} exited with {}: {}",
        binary.display(),
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(models_from_listing(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// Construct the backend for `agent`, honoring any CLI-path overrides. Errors on an unknown agent.
fn build_backend(agent: &str, paths: &BackendCliPaths) -> anyhow::Result<AnyBackend> {
    let backend = match agent.trim() {
        "claude" => AnyBackend::Claude(ClaudeCodeBackend::new()),
        "claude-acp" => AnyBackend::ClaudeAcp(match &paths.claude_acp_cli_path {
            Some(p) => ClaudeAcpBackend::with_agent_path(p.clone()),
            None => ClaudeAcpBackend::new(),
        }),
        "cursor" => AnyBackend::Cursor(match &paths.cursor_cli_path {
            Some(p) => CursorBackend::with_path(p.clone()),
            None => CursorBackend::new(),
        }),
        "codex" => AnyBackend::Codex(CodexBackend::new()),
        "codex-acp" => AnyBackend::CodexAcp(match &paths.codex_acp_cli_path {
            Some(p) => CodexAcpBackend::with_agent_path(p.clone()),
            None => CodexAcpBackend::new(),
        }),
        "opencode" => AnyBackend::OpenCode(match &paths.opencode_cli_path {
            Some(p) => OpenCodeAcpBackend::with_agent_path(p.clone()),
            None => OpenCodeAcpBackend::new(),
        }),
        "stub" => AnyBackend::Stub(StubBackend::new()),
        other => anyhow::bail!("unknown agent {:?}", other),
    };
    Ok(backend)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{cursor_cli_models, BackendModel};

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

    #[test]
    fn list_models_cursor_cli_agent_returns_curated_catalog() {
        // Given / When
        let catalog = cursor_cli_models();
        let json = render_models_json(&catalog);

        // Then
        assert!(json.contains("gpt-5.3-codex"));
        assert_eq!(catalog.default_model, "claude-4.6-sonnet-medium-thinking");
        assert!(catalog.models.len() >= 3);
    }
}
