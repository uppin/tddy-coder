//! `tddy-session-driver`: run one agent session on a target and serve MCP on stdin/stdout.
//!
//! stdout carries MCP frames and nothing else; every diagnostic goes to stderr.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context as _};
use clap::Parser;
use rmcp::ServiceExt as _;
use tddy_agent_launch::harness_from_spec::{harness_template_params, HarnessLaunch};
use tddy_agent_launch::host_coding_agents::host_coding_agents;
use tddy_session_driver::server::DriverServer;
use tddy_session_driver::session::{DriverSession, LaunchSpec};
use tddy_session_driver::target::DriverTarget;

/// Run one agent session on a local, ssh or Coder target through the daemon's own launch code,
/// and drive its terminal over MCP on stdin/stdout.
#[derive(Debug, Parser)]
#[command(name = "tddy-session-driver")]
struct Args {
    /// Where the agent runs: `local`, `ssh:<alias>`, or `coder:<owner>/<workspace>[.<agent>]`.
    /// A Coder target reads its deployment and token from CODER_URL and CODER_SESSION_TOKEN.
    #[arg(long, value_parser = |value: &str| value.parse::<DriverTarget>())]
    target: DriverTarget,

    /// The session's worktree, on the target: the agent's working directory. It must exist.
    #[arg(long)]
    worktree: PathBuf,

    /// The coding agent's id, resolved as the daemon resolves it (e.g. `claude`).
    #[arg(long)]
    agent: String,

    /// The model the agent is started on.
    #[arg(long)]
    model: Option<String>,

    /// The first turn sent to the agent.
    #[arg(long)]
    prompt: Option<String>,

    /// The agent's permission mode, as a session start passes it.
    #[arg(long)]
    permission_mode: Option<String>,

    /// The agent's binary on the target, instead of the path this machine resolves — which only
    /// a local target can run.
    #[arg(long)]
    binary: Option<PathBuf>,

    /// `KEY=VALUE` set on the agent, repeatable — on a remote target, its `HOME` and `PATH` there.
    #[arg(long = "env", value_parser = parse_env)]
    env: Vec<(String, String)>,

    /// The tddy home whose `tools/` the agents are read from (default `~/.tddy`).
    #[arg(long)]
    tddy_home: Option<PathBuf>,

    /// The `coder` CLI a Coder target is reached through.
    #[arg(long, default_value = "coder")]
    coder: PathBuf,

    /// How long the agent must stay up for its start to count, in seconds.
    #[arg(long, default_value_t = 5)]
    startup_window_secs: u64,

    /// The terminal's size, which the screen is read at.
    #[arg(long, default_value_t = 40)]
    rows: u16,
    #[arg(long, default_value_t = 120)]
    cols: u16,

    /// Launch the session before serving, instead of on the first `start_session`.
    #[arg(long)]
    start: bool,
}

fn parse_env(value: &str) -> Result<(String, String), String> {
    value
        .split_once('=')
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .ok_or_else(|| format!("{value} is not KEY=VALUE"))
}

fn session_spec(args: Args) -> anyhow::Result<(LaunchSpec, bool)> {
    let shell = args
        .target
        .shell(&args.coder, |name| std::env::var(name).ok())
        .map_err(|refusal| anyhow!(refusal))?;
    let tddy_home = match args.tddy_home {
        Some(home) => home,
        None => PathBuf::from(std::env::var("HOME").context("HOME is not set")?).join(".tddy"),
    };
    let mut agent = host_coding_agents(&tddy_home)?
        .resolve(&args.agent, None)
        .map_err(|error| anyhow!("agent {}: {error}", args.agent))?;
    if let Some(binary) = args.binary {
        agent.tool.binary = binary;
    }
    let session_id = uuid::Uuid::new_v4().to_string();
    let model = args.model.unwrap_or_default();
    let params = harness_template_params(&HarnessLaunch {
        model: &model,
        prompt: args.prompt.as_deref(),
        session_id: Some(&session_id),
        permission_mode: args.permission_mode.as_deref(),
        ..HarnessLaunch::default()
    });
    let session_dir = std::env::temp_dir()
        .join("tddy-session-driver")
        .join(&session_id);
    std::fs::create_dir_all(&session_dir)
        .with_context(|| format!("create the session directory {}", session_dir.display()))?;
    let session_dir = std::fs::canonicalize(&session_dir)?;
    Ok((
        LaunchSpec {
            agent,
            params,
            worktree: args.worktree,
            session_dir,
            shell,
            env: args.env,
            session_id,
            model,
            startup_window: Duration::from_secs(args.startup_window_secs),
            rows: args.rows,
            cols: args.cols,
        },
        args.start,
    ))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (spec, start) = match session_spec(Args::parse()) {
        Ok(spec) => spec,
        Err(error) => {
            eprintln!("tddy-session-driver: {error:#}");
            std::process::exit(1);
        }
    };
    let session = Arc::new(DriverSession::new(spec));
    if start {
        if let Err(failure) = session.start().await {
            eprintln!("tddy-session-driver: the session did not start: {failure}");
            std::process::exit(1);
        }
    }
    let server = DriverServer::new(Arc::clone(&session))
        .serve(rmcp::transport::stdio())
        .await?;
    server.waiting().await?;
    // The client is gone: end the session rather than leave its agent running.
    let _ = session.stop().await;
    Ok(())
}
