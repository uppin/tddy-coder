//! `tddy-tools extensions` — the args live here, `tddy-tools` keeps only the `main.rs` arm that
//! routes to [`run`] (the `analyze_cli.rs` / `restructure_args.rs` pattern).

use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::{Args, Parser, Subcommand};

use crate::dir::ExtensionsDir;
use crate::install::{install_from_path, install_from_registry, list, remove, InstallOutcome};
use crate::registry::Registry;

#[derive(Parser)]
#[command(name = "extensions")]
pub struct ExtensionsArgs {
    #[command(subcommand)]
    pub command: ExtensionsCommand,
}

#[derive(Subcommand)]
pub enum ExtensionsCommand {
    /// Install `<id>[@<version>]` from Zed's registry, or `--path <dir>` (a dev extension).
    Install(InstallArgs),
    /// List installed extensions and what each provides.
    List,
    /// Remove an installed extension.
    Remove(RemoveArgs),
}

#[derive(Args)]
pub struct InstallArgs {
    /// `<id>` or `<id>@<version>`.
    #[arg(required_unless_present = "path", conflicts_with = "path")]
    pub spec: Option<String>,

    /// A local extension directory to install instead of downloading.
    #[arg(long)]
    pub path: Option<PathBuf>,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// The extension id.
    pub id: String,
}

/// Resolves the extensions dir and registry from the environment and runs the command.
pub async fn run(args: ExtensionsArgs) -> anyhow::Result<()> {
    let dir = ExtensionsDir::resolve()?;
    for line in execute(&dir, &Registry::from_env(), args.command).await? {
        println!("{line}");
    }
    Ok(())
}

/// Runs `command` against `dir` and `registry`, returning the lines `run` prints.
pub async fn execute(
    dir: &ExtensionsDir,
    registry: &Registry,
    command: ExtensionsCommand,
) -> anyhow::Result<Vec<String>> {
    Ok(match command {
        ExtensionsCommand::Install(install) => {
            let outcome = match install.path {
                Some(path) => install_from_path(dir, &path)?,
                None => {
                    let spec = install
                        .spec
                        .context("expected <id>[@<version>], or --path <dir>")?;
                    let (id, version) = split_spec(&spec)?;
                    install_from_registry(dir, registry, id, version).await?
                }
            };
            vec![describe_outcome(&outcome)]
        }
        ExtensionsCommand::List => {
            let installed = list(dir)?;
            if installed.is_empty() {
                vec!["no extensions installed".to_string()]
            } else {
                installed.iter().map(|extension| extension.line()).collect()
            }
        }
        ExtensionsCommand::Remove(remove_args) => {
            vec![format!("removed {}", remove(dir, &remove_args.id)?.id)]
        }
    })
}

/// `<id>` or `<id>@<version>`, split on the first `@`.
fn split_spec(spec: &str) -> anyhow::Result<(&str, Option<&str>)> {
    match spec.split_once('@') {
        None => Ok((spec, None)),
        Some((_, "")) => bail!("{spec}: the version after @ is empty"),
        Some((id, version)) => Ok((id, Some(version))),
    }
}

fn describe_outcome(outcome: &InstallOutcome) -> String {
    match outcome {
        InstallOutcome::Installed(entry) => format!("installed {} {}", entry.id, entry.version),
        InstallOutcome::Replaced {
            previous_version,
            entry,
        } => format!(
            "replaced {} {} with {}",
            entry.id, previous_version, entry.version
        ),
        InstallOutcome::AlreadyInstalled(entry) => format!(
            "{} {} is already installed; nothing changed",
            entry.id, entry.version
        ),
    }
}
