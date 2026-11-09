//! `tddy-tools list-models --agent <id>` — enumerate an agent's selectable models as JSON.
//!
//! The daemon shells out to this subcommand for `ListAgentModels` and parses the JSON. Which
//! models an agent offers is its spec's `coding_agent.models`; the binary that runs this
//! library reads the registry and injects the catalogue as plain data ([`AgentModels`]), so
//! this crate names no agent and reads no spec.

use std::path::PathBuf;

use anyhow::Context as _;
use clap::Parser;
use tddy_core::adapters::LaunchAdapters;
use tddy_core::backend::{render_models_json, BackendModel, BackendModels};

/// `tddy-tools list-models --agent <id>` — print an agent's selectable models as JSON.
#[derive(Parser)]
#[command(name = "list-models")]
pub struct ListModelsArgs {
    /// Agent id or a CLI session type (`<id>-cli`), which names its agent's catalogue.
    #[arg(long)]
    pub agent: String,

    /// The binary the agent's model-listing command runs, when the caller already resolved it.
    /// Absent: the binary name the injected catalogue carries, searched on `PATH` by the OS.
    #[arg(long)]
    pub binary_path: Option<PathBuf>,

    /// The host's `<tddy home>/tools` directory: the catalogue is the coding-agent registry loaded
    /// from the built-in specs with that directory's overrides applied, so an operator's
    /// `tools/<agent>.yaml` model list is the one listed. Absent: the built-in specs alone.
    #[arg(long)]
    pub tools_dir: Option<PathBuf>,
}

/// How one agent's models are known, injected by the binary from the agent's spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentModelsSource {
    /// The spec's curated list: `(id, label)` pairs and the default model id.
    Curated {
        models: Vec<(String, String)>,
        default_model: String,
    },
    /// The spec's `models.list_command` rendered with its binary, read through the named
    /// models adapter.
    Listed { argv: Vec<String>, adapter: String },
}

/// One agent's model catalogue, under every name that asks for it (its id and its `-cli`
/// session type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentModels {
    pub names: Vec<String>,
    pub source: AgentModelsSource,
}

/// Enumerate the models `args.agent` supports and print them as the JSON contract.
pub async fn run_list_models(args: &ListModelsArgs, catalog: &[AgentModels]) -> anyhow::Result<()> {
    let models = list_agent_models(&args.agent, args.binary_path.as_deref(), catalog).await?;
    println!("{}", render_models_json(&models));
    Ok(())
}

/// The catalogue of `agent` (an agent id or its `-cli` session type) from the injected `catalog`.
pub async fn list_agent_models(
    agent: &str,
    binary_path: Option<&std::path::Path>,
    catalog: &[AgentModels],
) -> anyhow::Result<BackendModels> {
    let agent = agent.trim();
    // The simulated-response test backend: no spec stands for it, and it is no coding agent.
    if agent == "stub" {
        return Ok(BackendModels {
            models: vec![BackendModel::new("stub", "Stub")],
            default_model: "stub".to_string(),
        });
    }
    let entry = catalog
        .iter()
        .find(|entry| entry.names.iter().any(|name| name == agent))
        .ok_or_else(|| anyhow::anyhow!("no coding agent spec for agent {agent:?}"))?;
    match &entry.source {
        AgentModelsSource::Curated {
            models,
            default_model,
        } => Ok(BackendModels {
            models: models
                .iter()
                .map(|(id, label)| BackendModel::new(id.clone(), label.clone()))
                .collect(),
            default_model: default_model.clone(),
        }),
        AgentModelsSource::Listed { argv, adapter } => {
            let (program, rest) = argv
                .split_first()
                .ok_or_else(|| anyhow::anyhow!("an agent's model listing renders no program"))?;
            // The caller's resolved binary replaces the rendered program when handed one.
            let program = binary_path.map_or_else(
                || program.clone(),
                |path| path.to_string_lossy().into_owned(),
            );
            let models_adapter = LaunchAdapters::builtin()
                .models(adapter)
                .ok_or_else(|| anyhow::anyhow!("no models adapter named {adapter}"))?;
            let output = tokio::process::Command::new(&program)
                .args(rest)
                .stdin(std::process::Stdio::null())
                .output()
                .await
                .with_context(|| format!("running {} {}", program, rest.join(" ")))?;
            anyhow::ensure!(
                output.status.success(),
                "{} {} exited with {}: {}",
                program,
                rest.join(" "),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            let models = models_adapter.parse_models(&String::from_utf8_lossy(&output.stdout));
            anyhow::ensure!(!models.models.is_empty(), "{program} returned no models");
            Ok(models)
        }
    }
}
