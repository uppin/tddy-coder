//! `tddy-tools list-models --agent <id>` — enumerate a backend's selectable models as JSON.
//!
//! The daemon shells out to this subcommand for `ListAgentModels` and parses the JSON. The
//! catalogue assembly and the JSON contract it renders live in
//! [`tddy_core::backend::model_catalog`], beside the backends they enumerate; what stays here is
//! the argument parsing and the line written to stdout. See
//! docs/ft/web/tool-session-model-selection.md.

use std::path::PathBuf;

use clap::Parser;
use tddy_core::backend::{render_models_json, resolve_agent_models, BackendCliPaths};

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

impl ListModelsArgs {
    /// The binary-path overrides these arguments carry, in the shape the catalogue takes them.
    fn cli_paths(&self) -> BackendCliPaths {
        BackendCliPaths {
            cursor_cli_path: self.cursor_cli_path.clone(),
            claude_acp_cli_path: self.claude_acp_cli_path.clone(),
            codex_acp_cli_path: self.codex_acp_cli_path.clone(),
        }
    }
}

/// Enumerate the models an agent supports and print them as the JSON contract.
pub async fn run_list_models(args: &ListModelsArgs) -> anyhow::Result<()> {
    let catalog = resolve_agent_models(&args.agent, &args.cli_paths()).await?;
    println!("{}", render_models_json(&catalog));
    Ok(())
}
