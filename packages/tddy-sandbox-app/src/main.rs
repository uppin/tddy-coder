//! Standalone terminal app: spawn darwin sandbox + Claude, attach via SessionChannel gRPC.
//!
//! No host `tddy-daemon` is required. The host process:
//! 1. Spawns `sandbox-exec` → `tddy-sandbox-runner` (in-jail gRPC + Claude PTY + tddy-tools MCP)
//! 2. Dials the sandbox `SessionChannel` on loopback
//! 3. Proxies your terminal stdin/stdout and relays tool calls + HTTP egress on the host
//!
//! ```bash
//! tddy-sandbox-app --repo /path/to/git/checkout --model claude-opus-4-8
//! ```

mod bridge;
mod spawn;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use spawn::{spawn_claude_sandbox, SpawnParams};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_task::TaskRegistry;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "tddy-sandbox-app",
    about = "Spawn sandboxed Claude Code and attach your terminal (no host daemon)"
)]
struct Args {
    /// Git checkout to use as the sandbox worktree (tools run against this tree on the host).
    #[arg(long)]
    repo: PathBuf,

    /// Base directory for session metadata (default: `$HOME/.tddy`).
    #[arg(long, env = "TDDY_SESSION_BASE")]
    session_base: Option<PathBuf>,

    /// Claude model passed to the in-jail `claude` binary.
    #[arg(long, default_value = "claude-opus-4-8")]
    model: String,

    /// Claude permission mode (e.g. auto, bypassPermissions, plan).
    #[arg(long)]
    permission_mode: Option<String>,

    /// Path to the `claude` binary (default: `claude` on PATH).
    #[arg(long)]
    claude_binary: Option<String>,

    /// Path to `tddy-tools` for in-jail MCP (default: sibling of this binary).
    #[arg(long)]
    tddy_tools_path: Option<String>,

    /// Path to `tddy-sandbox-runner` (default: sibling of this binary).
    #[arg(long)]
    sandbox_runner_path: Option<String>,

    /// Working directory for Claude inside the jail (default: the mounted repo root).
    #[arg(long)]
    cwd: Option<PathBuf>,

    /// Enable debug logging for tddy sandbox components (HTTP/gRPC frame traces stay quiet).
    #[arg(short, long)]
    verbose: bool,
}

/// Default `RUST_LOG` when `--verbose` is set and the env var is unset.
const VERBOSE_RUST_LOG: &str = "\
    info,\
    tddy_sandbox_app=debug,\
    tddy_daemon::sandbox_session=debug,\
    tddy_sandbox_darwin=debug,\
    hyper=warn,\
    hyper_util=warn,\
    h2=warn,\
    tower=warn,\
    tonic=warn";

fn default_session_base() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".tddy")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.verbose && std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", VERBOSE_RUST_LOG);
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let session_id = Uuid::now_v7().to_string();
    let session_base = args.session_base.unwrap_or_else(default_session_base);
    let session_dir = session_base.join(SESSIONS_SUBDIR).join(&session_id);
    eprintln!("session_id={session_id}");
    eprintln!("session_dir={}", session_dir.display());
    eprintln!(
        "logs: {}/spawn.trace.log (host steps), {}/egress/ (in-jail runner after spawn)",
        session_dir.display(),
        session_dir.display()
    );
    if args.verbose {
        eprintln!("verbose logging enabled (RUST_LOG)");
    }

    let spawned = tokio::select! {
        res = spawn_claude_sandbox(SpawnParams {
            repo: args.repo,
            session_id: session_id.clone(),
            model: args.model,
            permission_mode: args.permission_mode.unwrap_or_else(|| "auto".to_string()),
            claude_binary: args.claude_binary,
            tddy_tools_path: args.tddy_tools_path,
            sandbox_runner_path: args.sandbox_runner_path,
            session_dir: session_dir.clone(),
            cwd: args.cwd,
        }) => res?,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("interrupted");
            std::process::exit(130);
        }
    };

    let task_registry = TaskRegistry::new();
    let bridge_result = bridge::run_terminal_bridge(
        &spawned.ready_marker,
        &spawned.session_id,
        &spawned.worktree_path,
        task_registry,
    )
    .await;

    log::info!(target: "tddy_sandbox_app", "stopping sandbox session {session_id}");
    let mut handle = spawned.handle;
    if let Err(e) = handle.child_mut().kill() {
        log::warn!(target: "tddy_sandbox_app", "kill sandbox child: {e}");
    }
    let _ = handle.child_mut().wait();

    if let Err(e) = bridge_result {
        spawn::log_spawn_diagnostics(&spawned.egress_dir, &spawned.session_dir);
        return Err(e);
    }

    Ok(())
}
