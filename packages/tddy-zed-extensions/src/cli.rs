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
    /// Let an installed extension's `extension.wasm` name its language servers' commands.
    EnableWasm(WasmArgs),
    /// Stop running an installed extension's `extension.wasm` (the default).
    DisableWasm(WasmArgs),
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

#[derive(Args)]
pub struct WasmArgs {
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn the_wasm_command_of(argv: &[&str]) -> Option<(&'static str, String)> {
        match ExtensionsArgs::try_parse_from(argv).ok()?.command {
            ExtensionsCommand::EnableWasm(args) => Some(("enable", args.id)),
            ExtensionsCommand::DisableWasm(args) => Some(("disable", args.id)),
            _ => None,
        }
    }

    #[test]
    fn enable_wasm_and_disable_wasm_take_the_extension_id() {
        // Given the two opt-in commands as a user types them
        let enable = ["extensions", "enable-wasm", "elixir"];
        let disable = ["extensions", "disable-wasm", "elixir"];

        // When they are parsed
        let parsed = (the_wasm_command_of(&enable), the_wasm_command_of(&disable));

        // Then each names its extension
        assert_eq!(
            parsed,
            (
                Some(("enable", "elixir".to_string())),
                Some(("disable", "elixir".to_string()))
            )
        );
    }
}
