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
use spawn::{resolve_codebase_mode, spawn_claude_sandbox, SpawnParams, SubagentSpawnConfig};
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

    /// Persistent jail `$HOME`, mounted read-write and reused across sandbox restarts (settings,
    /// session history, credentials). Kept separate from the real `~/.claude`.
    /// Default: `$HOME/.tddy/sandbox-claude-home`.
    ///
    /// Deliberately shared across all `tddy-sandbox-app` invocations on this host, not
    /// per-session — mirrors how a real user's `~/.claude` is shared across concurrent `claude`
    /// CLI sessions today; this is intentional, not an oversight.
    #[arg(long, env = "TDDY_SANDBOX_CLAUDE_HOME")]
    claude_home_dir: Option<PathBuf>,

    /// Remote-codebase mode: don't mount `--repo` into the jail. Claude sees only the
    /// (read-only) context dir and the persistent home; the real repo is reachable only via
    /// `mcp__tddy-tools__*` calls, which the host relays against the real `--repo` path. Matches
    /// the daemon's sandboxed-session isolation model (see docs/ft/daemon/remote-codebase-mode.md).
    /// Deprecated: prefer `--codebase-mode managed`, which this remains a working alias for.
    #[arg(long)]
    remote_codebase: bool,

    /// Codebase mode: `mounted` (default) mounts `--repo` read-write into the jail; `managed`
    /// keeps the repo unmounted, reaching it only via `mcp__tddy-tools__*` calls relayed by the
    /// host. Supersedes `--remote-codebase` (still accepted as a working alias).
    #[arg(long)]
    codebase_mode: Option<String>,

    /// Discovery subagent to wire into the session (e.g. `fastcontext`). When set, Claude gains
    /// the `subagent_new_session`/`subagent_prompt`/`subagent_cancel` MCP tools (see
    /// docs/ft/coder/managed-codebase-subagents.md).
    #[arg(long)]
    discovery_subagent: Option<String>,

    /// FastContext discovery-subagent endpoint (default: `http://localhost:30000`).
    #[arg(long)]
    fastcontext_url: Option<String>,

    /// FastContext discovery-subagent model id (default: `microsoft/FastContext-1.0-4B-RL`).
    #[arg(long)]
    fastcontext_model: Option<String>,

    /// FastContext discovery-subagent per-prompt turn budget (default: 10).
    #[arg(long)]
    fastcontext_max_turns: Option<u32>,

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

fn default_claude_home_dir() -> PathBuf {
    default_session_base().join("sandbox-claude-home")
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

    let claude_home_dir = args.claude_home_dir.unwrap_or_else(default_claude_home_dir);
    eprintln!(
        "claude_home_dir={} (persistent across restarts)",
        claude_home_dir.display()
    );

    let managed_codebase =
        resolve_codebase_mode(args.codebase_mode.as_deref(), args.remote_codebase)
            .map_err(|e| anyhow::anyhow!(e))?;
    if managed_codebase {
        eprintln!(
            "codebase_mode=managed: repo not mounted; Claude reaches it only via mcp__tddy-tools__* calls"
        );
    }
    let subagent = SubagentSpawnConfig {
        discovery_subagent: args.discovery_subagent,
        fastcontext_url: args.fastcontext_url,
        fastcontext_model: args.fastcontext_model,
        fastcontext_max_turns: args.fastcontext_max_turns,
    };
    if let Some(ref name) = subagent.discovery_subagent {
        eprintln!("discovery_subagent={name}");
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
            claude_home_dir,
            remote_codebase: managed_codebase,
            subagent,
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
