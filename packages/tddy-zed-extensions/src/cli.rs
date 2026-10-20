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
    match args.command {
        ExtensionsCommand::Install(install) => {
            let outcome = match install.path {
                Some(path) => install_from_path(&dir, &path)?,
                None => {
                    let spec = install
                        .spec
                        .context("expected <id>[@<version>], or --path <dir>")?;
                    let (id, version) = split_spec(&spec)?;
                    install_from_registry(&dir, &Registry::from_env(), id, version).await?
                }
            };
            println!("{}", describe_outcome(&outcome));
        }
        ExtensionsCommand::List => {
            let installed = list(&dir)?;
            if installed.is_empty() {
                println!("no extensions installed");
            } else {
                for extension in &installed {
                    println!("{}", extension.line());
                }
            }
        }
        ExtensionsCommand::Remove(remove_args) => {
            let removed = remove(&dir, &remove_args.id)?;
            println!("removed {}", removed.id);
        }
    }
    Ok(())
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
