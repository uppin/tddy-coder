//! The `spawn-conversation` CLI family: start a session through the universal start, and follow or
//! cancel that start by its `start_id`, each relayed to the daemon over `TDDY_SOCKET`.

use crate::cli::output_error;
use crate::cli::read_input;
use anyhow::Result;
use clap::Parser;
use tddy_core::toolcall::dispatch_toolcall;

/// Start a session in this session's project, linked to it (grill-me handoff): `StartSession`'s
/// arguments.
#[derive(Parser)]
#[command(name = "spawn_conversation")]
pub struct SpawnConversationArgs {
    /// JSON payload: `StartSession`'s arguments, e.g.
    /// `{"session_type":"claude-cli","model":"…","initial_prompt":"…","new_branch_name":"…"}`
    #[arg(long)]
    pub data: Option<String>,
}

/// Read a conversation's start (`spawn_conversation`'s `start_id`).
#[derive(Parser)]
#[command(name = "spawn_conversation_status")]
pub struct SpawnConversationStatusArgs {
    /// JSON payload: `{"start_id":"..."}`
    #[arg(long)]
    pub data: Option<String>,
}

/// Cancel a conversation's start (`spawn_conversation`'s `start_id`).
#[derive(Parser)]
#[command(name = "spawn_conversation_cancel")]
pub struct SpawnConversationCancelArgs {
    /// JSON payload: `{"start_id":"..."}`
    #[arg(long)]
    pub data: Option<String>,
}

pub async fn run_spawn_conversation(args: SpawnConversationArgs) -> Result<()> {
    let json_str = read_input(&args.data, false)?;
    let start: tddy_tool_engine::start_session_args::StartSessionArgs =
        serde_json::from_str(&json_str).map_err(|e| {
            output_error(&format!("invalid spawn_conversation arguments: {e}"), 1);
            anyhow::anyhow!("invalid spawn_conversation arguments")
        })?;
    relay_conversation_request(
        "spawn_conversation",
        serde_json::json!({ "type": "start-conversation", "args": start }),
    )
    .await
}

pub async fn run_spawn_conversation_status(args: SpawnConversationStatusArgs) -> Result<()> {
    let start_id = start_id_of(&args.data)?;
    relay_conversation_request(
        "spawn_conversation_status",
        crate::server::conversation_start_request_json("conversation-start-status", &start_id),
    )
    .await
}

pub async fn run_spawn_conversation_cancel(args: SpawnConversationCancelArgs) -> Result<()> {
    let start_id = start_id_of(&args.data)?;
    relay_conversation_request(
        "spawn_conversation_cancel",
        crate::server::conversation_start_request_json("conversation-start-cancel", &start_id),
    )
    .await
}

/// The `start_id` of `{"start_id":"..."}`.
fn start_id_of(data: &Option<String>) -> Result<String> {
    let json_str = read_input(data, false)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
        output_error(&format!("invalid JSON: {}", e), 1);
        e
    })?;
    parsed
        .get("start_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            output_error("missing or invalid 'start_id' string", 2);
            anyhow::anyhow!("missing start_id")
        })
}

/// Relay `request` to the daemon over `TDDY_SOCKET` and print its answer.
async fn relay_conversation_request(tool: &str, request: serde_json::Value) -> Result<()> {
    let Some(socket_path) = std::env::var_os("TDDY_SOCKET") else {
        output_error(&format!("TDDY_SOCKET not set; {tool} not relayed"), 1);
        return Ok(());
    };
    let response_json = dispatch_toolcall(std::path::Path::new(&socket_path), request)
        .await
        .map_err(|e| {
            output_error(&e, 1);
            anyhow::anyhow!(e)
        })?;
    println!("{}", response_json);
    Ok(())
}
