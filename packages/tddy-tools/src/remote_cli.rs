//! `tddy-tools remote` subcommand: helpers for remote-codebase mode.
//!
//! `remote list-tools` — reads the relay discovery file for the port, contacts the relay
//! daemon via the Connect-protocol `ListExecTools` RPC, and prints the tool catalog to stdout.
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
    /// (default: `~/.tddy/relay/`). Contacts the relay daemon via HTTP Connect RPC and prints
    /// the tool names. If no relay is running, prints a clear error to stderr and exits non-zero.
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
    /// Pulls remote context files (CLAUDE.md, etc.) from the relay and writes them to `--dest`.
    SyncContext(SyncContextArgs),
}

/// Args for `remote list-tools`.
#[derive(Parser)]
pub struct ListToolsArgs {
    /// Base directory for the relay discovery file. Defaults to `TDDY_RELAY_BASE_DIR` or `~/.tddy/relay/`.
    #[arg(long, value_name = "DIR")]
    pub base_dir: Option<PathBuf>,

    /// Session token for the request.
    #[arg(long, value_name = "TOKEN")]
    pub session_token: Option<String>,
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

    /// Session token for authentication.
    #[arg(long, value_name = "TOKEN")]
    pub session_token: Option<String>,
}

/// Args for `remote sync-context`.
#[derive(Parser)]
pub struct SyncContextArgs {
    /// Base directory for the relay discovery file. Defaults to `TDDY_RELAY_BASE_DIR` or `~/.tddy/relay/`.
    #[arg(long, value_name = "DIR")]
    pub base_dir: Option<PathBuf>,

    /// Destination directory to write synced context files into.
    #[arg(long, value_name = "DIR")]
    pub dest: Option<PathBuf>,

    /// Session token for the request.
    #[arg(long, value_name = "TOKEN")]
    pub session_token: Option<String>,
}

pub async fn run_remote(args: RemoteArgs) -> Result<()> {
    match args.subcommand {
        RemoteSubcommand::ListTools(a) => run_list_tools(a).await,
        RemoteSubcommand::StartSession(a) => run_start_session(a).await,
        RemoteSubcommand::ConnectSession(a) => run_connect_session(a).await,
        RemoteSubcommand::SyncContext(a) => run_sync_context(a).await,
    }
}

/// POST to `{base_url}/connection.ConnectionService/{method}` with a JSON body.
///
/// Uses the Connect-protocol transport: `POST /rpc/{service}/{method}` with
/// `content-type: application/json`. Returns the parsed JSON response body.
async fn connect_post(
    base_url: &str,
    method: &str,
    body: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}/connection.ConnectionService/{}", base_url, method);
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("HTTP POST to relay daemon failed: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "relay daemon returned HTTP {} for {}: {}",
            status,
            url,
            text
        );
    }
    let val = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| anyhow::anyhow!("failed to parse relay daemon response as JSON: {}", e))?;
    Ok(val)
}

/// Read the relay discovery file and return the base URL for Connect RPC calls.
fn resolve_base_url(base_dir: Option<PathBuf>) -> anyhow::Result<String> {
    let base_dir = resolve_base_dir(base_dir);
    let discovery_path = base_dir.join("daemon.json");

    if !discovery_path.exists() {
        anyhow::bail!(
            "no relay discovery file found at {}. \
             Start a tddy-daemon relay first or set TDDY_RELAY_BASE_DIR.",
            discovery_path.display()
        );
    }

    let content = std::fs::read_to_string(&discovery_path).map_err(|e| {
        anyhow::anyhow!(
            "could not read relay discovery file {}: {}",
            discovery_path.display(),
            e
        )
    })?;

    let discovery: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        anyhow::anyhow!(
            "relay discovery file {} is not valid JSON: {}",
            discovery_path.display(),
            e
        )
    })?;

    let port = discovery
        .get("port")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "relay discovery file {} does not contain a 'port' field.",
                discovery_path.display()
            )
        })? as u16;

    Ok(format!("http://127.0.0.1:{}/rpc", port))
}

async fn run_list_tools(args: ListToolsArgs) -> Result<()> {
    let base_url = match resolve_base_url(args.base_dir) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let token = args.session_token.as_deref().unwrap_or("");
    let body = serde_json::json!({
        "sessionToken": token,
        "daemonInstanceId": ""
    });

    let resp = match connect_post(&base_url, "ListExecTools", body).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    // Response shape: {"tools":[{"name":"Read","description":"...","inputSchemaJson":"{}"},...]}
    let tools = resp.get("tools").and_then(|v| v.as_array());

    if let Some(tools) = tools {
        for tool in tools {
            if let Some(name) = tool.get("name").and_then(|v| v.as_str()) {
                println!("{}", name);
            }
        }
    } else {
        // Fallback: print raw response
        println!("{}", resp);
    }

    Ok(())
}

async fn run_start_session(args: StartSessionArgs) -> Result<()> {
    let base_url = match resolve_base_url(args.base_dir) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let token = args.session_token.as_deref().unwrap_or("");
    let body = serde_json::json!({
        "sessionToken": token,
        "sessionType": "workspace",
        "daemonInstanceId": ""
    });

    let resp = match connect_post(&base_url, "StartSession", body).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    println!("{}", resp);
    Ok(())
}

async fn run_connect_session(args: ConnectSessionArgs) -> Result<()> {
    let base_url = match resolve_base_url(args.base_dir) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let token = args.session_token.as_deref().unwrap_or("");
    let session_id = args.session_id.as_deref().unwrap_or("");
    let body = serde_json::json!({
        "sessionId": session_id,
        "sessionToken": token
    });

    let resp = match connect_post(&base_url, "ConnectSession", body).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    println!("{}", resp);
    Ok(())
}

async fn run_sync_context(args: SyncContextArgs) -> Result<()> {
    let base_url = match resolve_base_url(args.base_dir) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let token = args.session_token.as_deref().unwrap_or("");
    // Fetch CLAUDE.md from the remote codebase via the ExecuteTool Read RPC.
    let body = serde_json::json!({
        "sessionToken": token,
        "sessionId": "",
        "toolName": "Read",
        "argsJson": r#"{"path":"CLAUDE.md"}"#,
        "daemonInstanceId": ""
    });

    let resp = match connect_post(&base_url, "ExecuteTool", body).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    // Write the result to dest directory.
    let dest = args
        .dest
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    if let Err(e) = std::fs::create_dir_all(&dest) {
        eprintln!(
            "error: could not create dest directory {}: {}",
            dest.display(),
            e
        );
        std::process::exit(1);
    }

    // Extract resultJson content and write it as CLAUDE.md in dest.
    let fallback = resp.to_string();
    let content = resp
        .get("resultJson")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback.as_str());
    let out_path = dest.join("CLAUDE.md");
    if let Err(e) = std::fs::write(&out_path, content) {
        eprintln!("error: could not write {}: {}", out_path.display(), e);
        std::process::exit(1);
    }

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
