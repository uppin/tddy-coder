//! `tddy-tools extensions` — the args live here, `tddy-tools` keeps only the `main.rs` arm that
//! routes to [`run`] (the `analyze_cli.rs` / `restructure_args.rs` pattern).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

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
    let _ = args;
    // TODO(reshape-extension-install): implement
    todo!("extensions cli run")
}
