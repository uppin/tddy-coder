//! tddy-tools: Generic tool calling handler for tddy-coder.
//!
//! - CLI mode (default): `submit` and `ask` subcommands relay to tddy-coder via Unix socket
//! - MCP mode (`--mcp`): Retains approval_prompt MCP server for backwards compatibility

mod cli;
mod server;

use anyhow::Result;
use clap::Parser;
use rmcp::ServiceExt;

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

    /// Output JSON schema for a goal. Use -o to write to file.
    GetSchema(cli::GetSchemaArgs),

    /// List registered workflow goals (JSON on stdout).
    ListSchemas(cli::ListSchemasArgs),
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
        Some(Subcommand::Submit(s)) => cli::run_submit(s)?,
        Some(Subcommand::Ask(s)) => cli::run_ask(s)?,
        Some(Subcommand::GetSchema(s)) => cli::run_get_schema(s)?,
        Some(Subcommand::ListSchemas(s)) => cli::run_list_schemas(s)?,
        None => {
            eprintln!("Error: missing subcommand. Use --help for usage.");
            std::process::exit(2);
        }
    }
    Ok(())
}

async fn run_mcp_server() -> Result<()> {
    let service = server::PermissionServer::new();
    let server = service.serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
