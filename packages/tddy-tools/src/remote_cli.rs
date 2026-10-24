//! `tddy-tools remote` subcommand: helpers for remote-codebase mode.
//!
//! `remote list-tools` — reads the relay discovery file for the port, contacts the relay
//! daemon via the Connect-protocol `ListExecTools` RPC, and prints the tool catalog to stdout.
//! When no relay is running, prints a clear error to stderr and exits non-zero (no panic).

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tddy_service::proto::exec_tools::{
    ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest, ListExecToolsResponse, ToolDef,
};
use tddy_service::proto::session::{
    BranchConflict, ConnectSessionRequest, ConnectSessionResponse, StartSessionRequest,
    StartSessionResponse,
};
use tddy_service::proto::types::BranchSession;

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
    /// Reads the relay discovery file (`daemon.json`) from `--base-dir`
    /// (default: `~/.tddy/relay/` in release, `tmp/.tddy/relay/` in debug). Contacts the relay daemon via HTTP Connect RPC and prints
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
    /// Base directory for the relay discovery file. Defaults to `~/.tddy/relay/` (release) or `tmp/.tddy/relay/` (debug).
    #[arg(long, value_name = "DIR")]
    pub base_dir: Option<PathBuf>,

    /// Session token for the request.
    #[arg(long, value_name = "TOKEN")]
    pub session_token: Option<String>,
}

/// Args for `remote start-session`.
#[derive(Parser)]
pub struct StartSessionArgs {
    /// Base directory for the relay discovery file. Defaults to `~/.tddy/relay/` (release) or `tmp/.tddy/relay/` (debug).
    #[arg(long, value_name = "DIR")]
    pub base_dir: Option<PathBuf>,

    /// Session token to associate with the new session.
    #[arg(long, value_name = "TOKEN")]
    pub session_token: Option<String>,

    /// A coding agent the codebase daemon offers, as `name@daemon_instance_id`, to put on the
    /// workspace session's roster — it runs on the codebase host. Repeatable.
    #[arg(long = "agent", value_name = "AGENT_ID")]
    pub agents: Vec<String>,
}

/// Args for `remote connect-session`.
#[derive(Parser)]
pub struct ConnectSessionArgs {
    /// Base directory for the relay discovery file. Defaults to `~/.tddy/relay/` (release) or `tmp/.tddy/relay/` (debug).
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
    /// Base directory for the relay discovery file. Defaults to `~/.tddy/relay/` (release) or `tmp/.tddy/relay/` (debug).
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

/// POST `request` to `{base_url}/{service}/{method}` and decode the answer as `Resp`.
///
/// Uses the Connect-protocol unary transport: `POST /rpc/{service}/{method}` with
/// `content-type: application/proto`, the body being the protobuf encoding of the request. The
/// daemon decodes that body with the generated request type of the method, so the encoding is the
/// one the daemon's service reads.
async fn connect_post<Req, Resp>(
    base_url: &str,
    service: &str,
    method: &str,
    request: &Req,
) -> anyhow::Result<Resp>
where
    Req: prost::Message,
    Resp: prost::Message + Default,
{
    let url = format!("{}/{}/{}", base_url, service, method);
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("content-type", "application/proto")
        .body(request.encode_to_vec())
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
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("failed to read relay daemon response: {}", e))?;
    Resp::decode(&bytes[..])
        .map_err(|e| anyhow::anyhow!("failed to decode relay daemon response as protobuf: {}", e))
}

/// Read the relay discovery file and return the base URL for Connect RPC calls.
fn resolve_base_url(base_dir: Option<PathBuf>) -> anyhow::Result<String> {
    let base_dir = resolve_base_dir(base_dir);
    let discovery_path = base_dir.join("daemon.json");

    if !discovery_path.exists() {
        anyhow::bail!(
            "no relay discovery file found at {}. \
             Start a tddy-daemon relay first, or use --base-dir to specify the relay directory.",
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
    let request = ListExecToolsRequest {
        session_token: token.to_string(),
        ..Default::default()
    };

    let answer: ListExecToolsResponse = match connect_post(
        &base_url,
        "exec_tools.ExecToolService",
        "ListExecTools",
        &request,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    // Shape: {"tools":[{"name":"Read","description":"...","inputSchemaJson":"{}"},...]}
    let resp = list_tools_json(&answer);
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
    let request = StartSessionRequest {
        session_token: token.to_string(),
        session_type: "workspace".to_string(),
        specialized_agents: args.agents,
        ..Default::default()
    };

    let answer: StartSessionResponse = match connect_post(
        &base_url,
        "session.SessionService",
        "StartSession",
        &request,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    println!("{}", start_session_json(&answer));
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
    let request = ConnectSessionRequest {
        session_id: session_id.to_string(),
        session_token: token.to_string(),
    };

    let answer: ConnectSessionResponse = match connect_post(
        &base_url,
        "session.SessionService",
        "ConnectSession",
        &request,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    println!("{}", connect_session_json(&answer));
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
    let request = ExecuteToolRequest {
        session_token: token.to_string(),
        tool_name: "Read".to_string(),
        args_json: r#"{"path":"CLAUDE.md"}"#.to_string(),
        ..Default::default()
    };

    let answer: ExecuteToolResponse = match connect_post(
        &base_url,
        "exec_tools.ExecToolService",
        "ExecuteTool",
        &request,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    let resp = execute_tool_json(&answer);

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

// ── Connect JSON of a decoded response ───────────────────────────────────────────────────────────
//
// What each command prints, and the field it reads, is the JSON encoding of its response. That
// encoding is the canonical proto3 mapping: a field is written under its camelCase name only when
// it is not the proto3 default. The builders below write the same keys and leave out the same
// defaults, so the output is what the CLI printed before its requests and responses were protobuf.

/// One Connect-JSON object, built field by field.
#[derive(Default)]
struct ConnectJson(serde_json::Map<String, serde_json::Value>);

impl ConnectJson {
    /// A string field, written only when non-empty.
    fn text(mut self, name: &str, value: &str) -> Self {
        if !value.is_empty() {
            self.0.insert(name.to_string(), value.into());
        }
        self
    }

    /// A bool field, written only when true.
    fn flag(mut self, name: &str, value: bool) -> Self {
        if value {
            self.0.insert(name.to_string(), value.into());
        }
        self
    }

    /// A message field, written only when present.
    fn message(mut self, name: &str, value: Option<serde_json::Value>) -> Self {
        if let Some(value) = value {
            self.0.insert(name.to_string(), value);
        }
        self
    }

    /// A repeated field, written only when non-empty.
    fn repeated(mut self, name: &str, values: Vec<serde_json::Value>) -> Self {
        if !values.is_empty() {
            self.0
                .insert(name.to_string(), serde_json::Value::Array(values));
        }
        self
    }

    fn into_value(self) -> serde_json::Value {
        serde_json::Value::Object(self.0)
    }
}

fn list_tools_json(response: &ListExecToolsResponse) -> serde_json::Value {
    let tools = response.tools.iter().map(tool_def_json).collect();
    ConnectJson::default().repeated("tools", tools).into_value()
}

fn tool_def_json(tool: &ToolDef) -> serde_json::Value {
    ConnectJson::default()
        .text("name", &tool.name)
        .text("description", &tool.description)
        .text("inputSchemaJson", &tool.input_schema_json)
        .into_value()
}

fn start_session_json(response: &StartSessionResponse) -> serde_json::Value {
    ConnectJson::default()
        .text("sessionId", &response.session_id)
        .text("livekitRoom", &response.livekit_room)
        .text("livekitUrl", &response.livekit_url)
        .text("livekitServerIdentity", &response.livekit_server_identity)
        .message(
            "branchConflict",
            response.branch_conflict.as_ref().map(branch_conflict_json),
        )
        .into_value()
}

fn branch_conflict_json(conflict: &BranchConflict) -> serde_json::Value {
    ConnectJson::default()
        .text("branch", &conflict.branch)
        .message("owner", conflict.owner.as_ref().map(branch_session_json))
        .text("suggestedBranchName", &conflict.suggested_branch_name)
        .into_value()
}

fn branch_session_json(owner: &BranchSession) -> serde_json::Value {
    ConnectJson::default()
        .flag("exists", owner.exists)
        .text("sessionId", &owner.session_id)
        .flag("isActive", owner.is_active)
        .text("status", &owner.status)
        .into_value()
}

fn connect_session_json(response: &ConnectSessionResponse) -> serde_json::Value {
    ConnectJson::default()
        .text("livekitRoom", &response.livekit_room)
        .text("livekitUrl", &response.livekit_url)
        .text("livekitServerIdentity", &response.livekit_server_identity)
        .into_value()
}

fn execute_tool_json(response: &ExecuteToolResponse) -> serde_json::Value {
    ConnectJson::default()
        .text("resultJson", &response.result_json)
        .flag("isError", response.is_error)
        .text("errorMessage", &response.error_message)
        .text("jobId", &response.job_id)
        .flag("jobRunning", response.job_running)
        .into_value()
}

fn resolve_base_dir(override_dir: Option<PathBuf>) -> PathBuf {
    if let Some(d) = override_dir {
        return d;
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    tddy_core::output::default_tddy_data_dir()
        .map(|b| b.join("relay"))
        .unwrap_or_else(|| home.join(".tddy").join("relay"))
}
