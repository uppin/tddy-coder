//! `tddy-tools remote` subcommand: helpers for remote-codebase mode.
//!
//! `remote list-tools` — reads the relay discovery file for the port, contacts the relay
//! daemon via HTTP, and prints the tool catalog to stdout.
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
    /// (default: `~/.tddy/relay/`). Contacts the relay daemon via HTTP and prints the tool names.
    /// If no relay is running, prints a clear error to stderr and exits non-zero.
    ListTools(ListToolsArgs),

    /// Start a new remote session on the relay daemon.
    ///
    /// Creates a new codebase session on the relay and prints the session ID and token.
    StartSession(StartSessionArgs),

    /// Connect to an existing remote session on the relay daemon.
    ///
    /// Attaches to a running relay session identified by `--session-id`.
    ConnectSession(ConnectSessionArgs),

    /// Sync context keys from the relay daemon into the local workflow context.
    ///
    /// Pulls remote context (daemon URL, session token, etc.) from the relay discovery
    /// file and prints them as `KEY=VALUE` pairs.
    SyncContext(SyncContextArgs),
}

/// Args for `remote list-tools`.
#[derive(Parser)]
pub struct ListToolsArgs {
    /// Base directory for the relay discovery file. Defaults to `TDDY_RELAY_BASE_DIR` or `~/.tddy/relay/`.
    #[arg(long, value_name = "DIR")]
    pub base_dir: Option<PathBuf>,
}

/// Args for `remote start-session`.
#[derive(Parser)]
pub struct StartSessionArgs {
    /// Base directory for the relay discovery file. Defaults to `TDDY_RELAY_BASE_DIR` or `~/.tddy/relay/`.
    #[arg(long, value_name = "DIR")]
    pub base_dir: Option<PathBuf>,

    /// Session token to associate with the new session.
    #[arg(long, value_name = "TOKEN")]
    pub session_token: Option<String>,
}

/// Args for `remote connect-session`.
#[derive(Parser)]
pub struct ConnectSessionArgs {
    /// Base directory for the relay discovery file. Defaults to `TDDY_RELAY_BASE_DIR` or `~/.tddy/relay/`.
    #[arg(long, value_name = "DIR")]
    pub base_dir: Option<PathBuf>,

    /// ID of the session to connect to.
    #[arg(long, value_name = "SESSION_ID")]
    pub session_id: Option<String>,
}

/// Args for `remote sync-context`.
#[derive(Parser)]
pub struct SyncContextArgs {
    /// Base directory for the relay discovery file. Defaults to `TDDY_RELAY_BASE_DIR` or `~/.tddy/relay/`.
    #[arg(long, value_name = "DIR")]
    pub base_dir: Option<PathBuf>,
}

pub async fn run_remote(args: RemoteArgs) -> Result<()> {
    match args.subcommand {
        RemoteSubcommand::ListTools(a) => run_list_tools(a).await,
        RemoteSubcommand::StartSession(a) => run_start_session(a),
        RemoteSubcommand::ConnectSession(a) => run_connect_session(a),
        RemoteSubcommand::SyncContext(a) => run_sync_context(a),
    }
}

async fn run_list_tools(args: ListToolsArgs) -> Result<()> {
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

    let port = match discovery.get("port").and_then(|v| v.as_u64()) {
        Some(p) => p as u16,
        None => {
            eprintln!(
                "error: relay discovery file {} does not contain a 'port' field.",
                discovery_path.display()
            );
            std::process::exit(1);
        }
    };

    // Contact the relay daemon via HTTP to fetch the tool catalog.
    let url = format!("http://127.0.0.1:{}/rpc/list-tools", port);

    let tools_json: anyhow::Result<String> = async {
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP request to relay daemon failed: {}", e))?;
        if !resp.status().is_success() {
            anyhow::bail!(
                "relay daemon returned HTTP {} for {}",
                resp.status(),
                url
            );
        }
        let body = resp
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("failed to read relay daemon response: {}", e))?;
        Ok(body)
    }
    .await;

    let tools_json = match tools_json {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    // The relay daemon returns a JSON array of tool names — print each on its own line.
    let tools: Vec<serde_json::Value> = match serde_json::from_str(&tools_json) {
        Ok(v) => v,
        Err(_) => {
            // Not a JSON array — print the raw response as-is.
            println!("{}", tools_json);
            return Ok(());
        }
    };

    for tool in &tools {
        if let Some(name) = tool.as_str() {
            println!("{}", name);
        } else {
            println!("{}", tool);
        }
    }

    Ok(())
}

fn run_start_session(_args: StartSessionArgs) -> Result<()> {
    anyhow::bail!("start-session: not yet implemented")
}

fn run_connect_session(_args: ConnectSessionArgs) -> Result<()> {
    anyhow::bail!("connect-session: not yet implemented")
}

fn run_sync_context(_args: SyncContextArgs) -> Result<()> {
    anyhow::bail!("sync-context: not yet implemented")
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
