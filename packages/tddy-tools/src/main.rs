//! tddy-tools: Generic tool calling handler for tddy-coder.
//!
//! - CLI mode (default): `submit` and `ask` subcommands relay to tddy-coder via Unix socket
//! - MCP mode (`--mcp`): Retains approval_prompt MCP server for backwards compatibility

mod build_cli;
mod cli;
mod pty_relay;
mod remote_cli;
mod session_hook;

use anyhow::Result;
use clap::Parser;
use rmcp::ServiceExt;
use tddy_tools::server::PermissionServer;

#[derive(Parser)]
#[command(name = "tddy-tools")]
#[command(
    about = "Generic tool calling handler for tddy-coder: submit structured output, ask questions, or run MCP server"
)]
struct Args {
    /// Run as MCP server (stdio transport). Used by Claude Code --permission-prompt-tool.
    #[arg(long)]
    mcp: bool,

    #[command(subcommand)]
    subcommand: Option<Subcommand>,
}

#[derive(Parser)]
enum Subcommand {
    /// Submit structured output. Validates against schema, relays to tddy-coder.
    Submit(cli::SubmitArgs),

    /// Ask clarification questions. Blocks until user answers in TUI.
    Ask(cli::AskArgs),

    /// Transition the workflow state machine to another goal (agent-driven orchestration).
    /// Relays to tddy-coder; returns the next goal's instructions (or a rejection).
    Transition(cli::TransitionArgs),

    /// Output JSON schema for a goal. Use -o to write to file.
    GetSchema(cli::GetSchemaArgs),

    /// List registered workflow goals (JSON on stdout).
    ListSchemas(cli::ListSchemasArgs),

    /// Merge JSON into the active workflow session context (requires TDDY_SESSION_DIR / TDDY_WORKFLOW_SESSION_ID).
    SetSessionContext(cli::SetSessionContextArgs),

    /// Merge workflow/demo fields into changeset.yaml (validated JSON, atomic write).
    PersistChangesetWorkflow(cli::PersistChangesetWorkflowArgs),

    /// List action manifests (`actions/*.yaml`) for a session directory (machine-readable JSON).
    ListActions(cli::ListActionsArgs),

    /// Invoke a session action by id with JSON arguments (`--data`).
    InvokeAction(cli::InvokeActionArgs),

    /// List build targets from `BUILD.yaml` manifests (machine-readable JSON).
    BuildList(build_cli::BuildListArgs),

    /// Build a target from a `BUILD.yaml` manifest.
    Build(build_cli::BuildArgs),

    /// Spawn a command in a PTY and relay keyboard+output — same wiring as the daemon uses
    /// for claude-cli sessions. Also start/connect to daemon sessions (including sandbox):
    /// `pty-relay --daemon-url URL --project-id ID --sandbox`
    /// Example: tddy-tools pty-relay -- claude --model claude-opus-4-8
    PtyRelay(Box<pty_relay::PtyRelayArgs>),

    /// Remote codebase mode helpers: list-tools, etc.
    Remote(remote_cli::RemoteArgs),

    /// Report granular session activity status to the daemon (invoked by Claude Code hooks).
    /// Reads hook event JSON from stdin; fails quietly — always exits 0.
    SessionHook(session_hook::SessionHookArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();

    let args = Args::parse();

    if args.mcp {
        return run_mcp_server().await;
    }

    match args.subcommand {
        Some(Subcommand::Submit(s)) => cli::run_submit(s).await?,
        Some(Subcommand::Ask(s)) => cli::run_ask(s).await?,
        Some(Subcommand::Transition(s)) => cli::run_transition(s).await?,
        Some(Subcommand::GetSchema(s)) => cli::run_get_schema(s)?,
        Some(Subcommand::ListSchemas(s)) => cli::run_list_schemas(s)?,
        Some(Subcommand::SetSessionContext(s)) => cli::run_set_session_context(s)?,
        Some(Subcommand::PersistChangesetWorkflow(s)) => cli::run_persist_changeset_workflow(s)?,
        Some(Subcommand::ListActions(s)) => cli::run_list_actions(s).await?,
        Some(Subcommand::InvokeAction(s)) => cli::run_invoke_action(s).await?,
        Some(Subcommand::BuildList(s)) => build_cli::run_build_list(s).await?,
        Some(Subcommand::Build(s)) => build_cli::run_build(s).await?,
        Some(Subcommand::PtyRelay(s)) => pty_relay::run_pty_relay(*s).await?,
        Some(Subcommand::Remote(s)) => remote_cli::run_remote(s).await?,
        Some(Subcommand::SessionHook(s)) => session_hook::run_session_hook(s).await,
        None => {
            eprintln!("Error: missing subcommand. Use --help for usage.");
            std::process::exit(2);
        }
    }
    Ok(())
}

async fn run_mcp_server() -> Result<()> {
    let service = PermissionServer::new();
    let server = service.serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
