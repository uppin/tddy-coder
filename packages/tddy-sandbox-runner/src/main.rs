//! Run the in-jail sandbox gRPC server + claude PTY (`tddy-sandbox-runner` binary).
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use tddy_sandbox_runner::{run_sandbox_runner, run_workspace_tool_runner, SandboxRunnerArgs};

/// The runner's CLI: the arguments every mode shares, plus the one flag that selects the mode
/// which hosts no agent.
#[derive(Parser, Debug)]
struct Cli {
    #[command(flatten)]
    runner: SandboxRunnerArgs,
    /// Serve a sandboxed `workspace` session's tool calls against this worktree — the session's
    /// checkout, as mounted inside this jail — and host no agent: no PTY and no in-jail
    /// `tddy-tools --mcp` server. The host sends `in_jail_tool_request` over the `SessionChannel`
    /// and the runner answers `in_jail_tool_response`, the reverse of the
    /// `tool_request`/`tool_response` pair an in-jail agent uses to reach the host. Reached over
    /// `--stdio` by a host that pipes this process, or `--grpc-listen-port` by one that dials its
    /// loopback listener; one of the two is required. An egress shim exists only where
    /// `--egress-shim-port` asks for one: a jail holding just a checkout needs no network, a jail
    /// that also runs the build reaches it through the host's CONNECT relay.
    #[arg(long)]
    workspace_tools: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();
    let cli = Cli::parse();
    let result = match cli.workspace_tools {
        Some(worktree) => run_workspace_tool_runner(cli.runner, worktree).await,
        None => run_sandbox_runner(cli.runner).await,
    };
    if let Err(err) = result {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
    Ok(())
}
