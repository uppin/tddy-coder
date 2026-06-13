//! `tddy-tools remote` subcommand: helpers for remote-codebase mode.
//!
//! `remote list-tools` — reads the relay discovery file and prints the tool catalog as JSON.
//! When no relay is running, prints a clear error to stderr and exits non-zero (no panic).

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Remote-codebase mode subcommands.
#[derive(Parser)]
#[command(name = "remote")]
pub struct RemoteArgs {
    #[command(subcommand)]
    pub subcommand: RemoteSubcommand,
}

#[derive(Subcommand)]
pub enum RemoteSubcommand {
    /// List available remote tools from the relay daemon.
    ///
    /// Reads the relay discovery file (`daemon.json`) from `--base-dir` or `TDDY_RELAY_BASE_DIR`
    /// (default: `~/.tddy/relay/`). Prints a JSON array of tool names to stdout.
    /// If no relay is running, prints a clear error to stderr and exits non-zero.
    ListTools(ListToolsArgs),
}

/// Args for `remote list-tools`.
#[derive(Parser)]
pub struct ListToolsArgs {
    /// Base directory for the relay discovery file. Defaults to `TDDY_RELAY_BASE_DIR` or `~/.tddy/relay/`.
    #[arg(long, value_name = "DIR")]
    pub base_dir: Option<PathBuf>,
}

pub fn run_remote(args: RemoteArgs) -> Result<()> {
    match args.subcommand {
        RemoteSubcommand::ListTools(a) => run_list_tools(a),
    }
}

fn run_list_tools(args: ListToolsArgs) -> Result<()> {
    let base_dir = resolve_base_dir(args.base_dir);
    let discovery_path = base_dir.join("daemon.json");

    if !discovery_path.exists() {
        eprintln!(
            "error: no relay discovery file found at {}. \
             Start a tddy-daemon relay first or set TDDY_RELAY_BASE_DIR.",
            discovery_path.display()
        );
        std::process::exit(1);
    }

    let content = match std::fs::read_to_string(&discovery_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "error: could not read relay discovery file {}: {}",
                discovery_path.display(),
                e
            );
            std::process::exit(1);
        }
    };

    let discovery: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "error: relay discovery file {} is not valid JSON: {}",
                discovery_path.display(),
                e
            );
            std::process::exit(1);
        }
    };

    // Extract tool names from the discovery file, or return an empty array if not present.
    let tools = discovery
        .get("tools")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Print as a JSON array of tool names (strings or objects).
    let out = serde_json::Value::Array(tools);
    println!(
        "{}",
        serde_json::to_string(&out).unwrap_or_else(|_| "[]".to_string())
    );

    Ok(())
}

fn resolve_base_dir(override_dir: Option<PathBuf>) -> PathBuf {
    if let Some(d) = override_dir {
        return d;
    }
    if let Ok(env_dir) = std::env::var("TDDY_RELAY_BASE_DIR") {
        return PathBuf::from(env_dir);
    }
    // Default: ~/.tddy/relay/
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".tddy").join("relay");
    }
    PathBuf::from(".tddy/relay")
}
