//! tddy-tools: Generic tool calling handler for tddy-coder.
//!
//! - CLI mode (default): `submit` and `ask` subcommands relay to tddy-coder via Unix socket
//! - MCP mode (`--mcp`): Retains approval_prompt MCP server for backwards compatibility

mod cli;
mod index_client;
mod index_console;
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

    /// Start a new implementation conversation on a fresh worktree (grill-me handoff).
    SpawnConversation(cli::SpawnConversationArgs),

    /// List every tool available to the session as JSON (web Inspector → Tools panel).
    ListTools(cli::ListToolsArgs),

    /// Invoke a single session tool by name with JSON `--data` (web Inspector → Tools "invoke").
    CallTool(cli::CallToolArgs),

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
    BuildList(tddy_bsp::build_cli::BuildListArgs),

    /// Build a target from a `BUILD.yaml` manifest.
    Build(tddy_bsp::build_cli::BuildArgs),

    /// Spawn a command in a PTY and relay keyboard+output — same wiring as the daemon uses
    /// for claude-cli sessions. Also start/connect to daemon sessions (including sandbox):
    /// `pty-relay --daemon-url URL --project-id ID --sandbox`
    /// Example: tddy-tools pty-relay -- claude --model opus
    PtyRelay(Box<pty_relay::PtyRelayArgs>),

    /// Remote codebase mode helpers: list-tools, etc.
    Remote(remote_cli::RemoteArgs),

    /// Report granular session activity status to the daemon (invoked by Claude Code hooks).
    /// Reads hook event JSON from stdin; fails quietly — always exits 0.
    SessionHook(session_hook::SessionHookArgs),

    /// Enumerate the models an agent supports (JSON on stdout). Queries the underlying agent
    /// command where possible (cursor `--list-models`, ACP `available_models`), else a curated list.
    ListModels(tddy_tools::list_models::ListModelsArgs),

    /// Rust code analysis: coverage, CRAP report, duplicate-tests.
    Analyze(tddy_code_analysis::analyze_cli::AnalyzeArgs),

    /// Plan-driven Rust refactoring via rust-analyzer (through tddy-lsp).
    Restructure(tddy_code_restructuring::restructure_cli::RestructureArgs),
}

/// Initialise logging. When `TDDY_TOOLS_LOG_FILE` is set (e.g. by the sandbox runner, which points
/// it at the session egress dir), env_logger writes to that file — append mode — so an in-jail
/// `--mcp` server's logs (including specialized-subagent HTTP activity) are persisted where the
/// host can read them, instead of vanishing into the parent process's captured stderr. Otherwise
/// logs go to stderr as before. Never panics: a file that can't be opened falls back to stderr.
fn init_logging() {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"));
    if let Some(path) = std::env::var_os("TDDY_TOOLS_LOG_FILE") {
        if !path.is_empty() {
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                Ok(file) => {
                    builder.target(env_logger::Target::Pipe(Box::new(file)));
                }
                Err(e) => {
                    eprintln!("tddy-tools: cannot open TDDY_TOOLS_LOG_FILE {path:?}: {e}; logging to stderr");
                }
            }
        }
    }
    let _ = builder.try_init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    let args = Args::parse();

    if args.mcp {
        return run_mcp_server().await;
    }

    match args.subcommand {
        Some(Subcommand::Submit(s)) => cli::run_submit(s).await?,
        Some(Subcommand::Ask(s)) => cli::run_ask(s).await?,
        Some(Subcommand::SpawnConversation(s)) => cli::run_spawn_conversation(s).await?,
        Some(Subcommand::ListTools(s)) => cli::run_list_tools(s)?,
        Some(Subcommand::CallTool(s)) => cli::run_call_tool(s).await?,
        Some(Subcommand::Transition(s)) => cli::run_transition(s).await?,
        Some(Subcommand::GetSchema(s)) => cli::run_get_schema(s)?,
        Some(Subcommand::ListSchemas(s)) => cli::run_list_schemas(s)?,
        Some(Subcommand::SetSessionContext(s)) => cli::run_set_session_context(s)?,
        Some(Subcommand::PersistChangesetWorkflow(s)) => cli::run_persist_changeset_workflow(s)?,
        Some(Subcommand::ListActions(s)) => cli::run_list_actions(s).await?,
        Some(Subcommand::InvokeAction(s)) => cli::run_invoke_action(s).await?,
        Some(Subcommand::BuildList(s)) => tddy_bsp::build_cli::run_build_list(s).await?,
        Some(Subcommand::Build(s)) => tddy_bsp::build_cli::run_build(s).await?,
        Some(Subcommand::PtyRelay(s)) => pty_relay::run_pty_relay(*s).await?,
        Some(Subcommand::Remote(s)) => remote_cli::run_remote(s).await?,
        Some(Subcommand::SessionHook(s)) => session_hook::run_session_hook(s).await,
        Some(Subcommand::ListModels(s)) => tddy_tools::list_models::run_list_models(&s).await?,
        Some(Subcommand::Analyze(s)) => tddy_code_analysis::analyze_cli::run(s)?,
        Some(Subcommand::Restructure(s)) => index_client::run_restructure(s).await?,
        None => {
            eprintln!("Error: missing subcommand. Use --help for usage.");
            std::process::exit(2);
        }
    }
    Ok(())
}

async fn run_mcp_server() -> Result<()> {
    // The spawn seed is parsed before anything is served, and an unparseable value fails the start.
    // `SpecializedAgentDef` is `deny_unknown_fields`, so a `tddy-tools` older than the daemon that
    // wrote `TDDY_SUBAGENTS_JSON` lands here — and serving on with an empty seed would hand the
    // main agent back every tool the session's agents took over, with no line anywhere naming the
    // cause. A security control that turns itself off on version skew is worse than one that
    // refuses to start.
    let seed = tddy_discovery::roster::subagents_from_env().map_err(|e| anyhow::anyhow!(e))?;
    log::info!(
        target: "tddy_tools::server",
        "spawn seed carries {} specialized agent def(s)",
        seed.len()
    );
    // The roster the spawning daemon already read, when it handed one over: applied as a frame
    // would be, so the first answer is right even if the stream is slow, and refused like the def
    // seed when it does not decode — a start the daemon meant to carry a roster must not quietly
    // run without it.
    if let Some(roster) =
        tddy_discovery::roster::roster_seed_from_env().map_err(|e| anyhow::anyhow!(e))?
    {
        log::info!(
            target: "tddy_tools::server",
            "spawn hands over roster rev {} with {} agent(s)",
            roster.rev,
            roster.agents.len()
        );
        tddy_tools::session_agents::session_agent_roster().apply_snapshot(roster);
    }
    // Follow the session's agent roster for as long as this process serves MCP, telling the main
    // agent to re-list its tools on every revision (docs/ft/daemon/session-agent-roster.md § The
    // roster stream) — and wait for its first snapshot **before** serving. A client builds its tool
    // pool from its first `tools/list`, asked the moment the handshake completes; answered before
    // the roster was known, that list could not reach the session's agents and still offered every
    // tool they took over (session 01a0f089: a host-run agent told to "ask Gemma" found no way to).
    let announcer =
        std::sync::Arc::new(tddy_tools::tool_list_announcer::ToolListAnnouncer::default());
    let announces = announcer.clone();
    tddy_tools::session_agents::follow_session_agent_roster(move || announces.announce()).await;
    announcer.serving_starts();
    let service = PermissionServer::new();
    let server = service.serve(rmcp::transport::stdio()).await?;
    announcer.attach(server.peer().clone());
    server.waiting().await?;
    Ok(())
}
