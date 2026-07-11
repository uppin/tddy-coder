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
mod config;
mod spawn;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use clap::Parser;
use spawn::{resolve_codebase_mode, spawn_claude_sandbox, AgentKind, SpawnParams};
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

    /// Optional YAML config (schema: `config::SandboxAppConfig`). CLI flags override its values.
    /// Inline `subagents:` defs let you e.g. point `fastcontext` at a local Ollama server without
    /// a separate agents dir.
    #[arg(long, short = 'c')]
    config: Option<PathBuf>,

    /// Base directory for session metadata (default: `$HOME/.tddy`).
    #[arg(long, env = "TDDY_SESSION_BASE")]
    session_base: Option<PathBuf>,

    /// Agent kind: `claude` (default) or `cursor` (`agent` binary).
    #[arg(long, default_value = "claude")]
    agent_kind: String,

    /// Claude model passed to the in-jail `claude` binary (default: `claude-opus-4-8`).
    #[arg(long)]
    model: Option<String>,

    /// Claude permission mode (e.g. auto, bypassPermissions, plan).
    #[arg(long)]
    permission_mode: Option<String>,

    /// Path to the `claude` binary (default: `claude` on PATH).
    #[arg(long)]
    claude_binary: Option<String>,

    /// Path to the Cursor `agent` binary when `--agent-kind cursor` (default: `agent` on PATH).
    #[arg(long)]
    cursor_binary: Option<String>,

    /// Persistent jail `$HOME` for Cursor (`agent`). Default: `$HOME/.tddy/sandbox-cursor-home`.
    #[arg(long, env = "TDDY_SANDBOX_CURSOR_HOME")]
    cursor_home_dir: Option<PathBuf>,

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

    /// Specialized agent to wire into the session (e.g. `fastcontext`), repeatable for multiple
    /// agents. When set, Claude gains the `subagent_new_session`/`subagent_prompt`/
    /// `subagent_cancel` MCP tools (see docs/ft/coder/specialized-subagents.md).
    #[arg(long = "specialized-agent")]
    specialized_agent: Vec<String>,

    /// Directory to resolve named agents from, in addition to the builtins (default:
    /// `<session-base>/agents`).
    #[arg(long)]
    agents_dir: Option<PathBuf>,

    /// `RUST_LOG` for the in-jail `tddy-tools --mcp` server; its logs (incl. specialized subagent
    /// HTTP activity) are persisted to `<session-dir>/egress/tddy-tools.mcp.log`. Overrides the
    /// config's `mcp_log_level`.
    #[arg(long)]
    mcp_log_level: Option<String>,

    /// Enable debug logging for tddy sandbox components (HTTP/gRPC frame traces stay quiet).
    #[arg(short, long)]
    verbose: bool,

    /// Args after `--` are forwarded verbatim to the in-jail `claude`, appended after any
    /// `claude_args` from the config file (a trailing positional prompt therefore lands last).
    /// E.g. `-- --add-dir /extra "implement the feature"`.
    #[arg(last = true)]
    claude_args: Vec<String>,
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

fn default_cursor_home_dir() -> PathBuf {
    default_session_base().join("sandbox-cursor-home")
}

/// Repoint `<session-base>/sessions/latest` at `<session_id>` (best-effort; failures are ignored —
/// it's a convenience pointer for finding the current session's logs, never load-bearing).
fn update_latest_session_symlink(session_base: &std::path::Path, session_id: &str) {
    #[cfg(unix)]
    {
        let sessions_dir = session_base.join(SESSIONS_SUBDIR);
        if std::fs::create_dir_all(&sessions_dir).is_err() {
            return;
        }
        let link = sessions_dir.join("latest");
        let _ = std::fs::remove_file(&link);
        let _ = std::os::unix::fs::symlink(session_id, &link);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.verbose && std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", VERBOSE_RUST_LOG);
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Load the optional YAML config first; every CLI flag below overrides its config counterpart.
    let cfg = match args.config.as_deref() {
        Some(path) => config::SandboxAppConfig::load(path)?,
        None => config::SandboxAppConfig::default(),
    };

    let session_id = Uuid::now_v7().to_string();
    let session_base = args
        .session_base
        .or(cfg.session_base)
        .unwrap_or_else(default_session_base);
    let session_dir = session_base.join(SESSIONS_SUBDIR).join(&session_id);
    eprintln!("session_id={session_id}");
    eprintln!("session_dir={}", session_dir.display());
    // Best-effort convenience: repoint `<session-base>/sessions/latest` at this session so logs are
    // easy to find without copying the UUID (`tail -f ~/.tddy/sessions/latest/egress/*.log`).
    update_latest_session_symlink(&session_base, &session_id);
    eprintln!(
        "logs: {}/spawn.trace.log (host steps), {}/egress/ (in-jail runner after spawn)",
        session_dir.display(),
        session_dir.display()
    );
    if args.verbose {
        eprintln!("verbose logging enabled (RUST_LOG)");
    }

    let agent_kind = AgentKind::parse(&args.agent_kind).map_err(|e| anyhow::anyhow!(e))?;

    let claude_home_dir = args
        .claude_home_dir
        .or(cfg.claude_home_dir)
        .unwrap_or_else(default_claude_home_dir);
    let cursor_home_dir = args
        .cursor_home_dir
        .or(cfg.cursor_home_dir)
        .unwrap_or_else(default_cursor_home_dir);
    let persistent_home = match agent_kind {
        AgentKind::Claude => &claude_home_dir,
        AgentKind::Cursor => &cursor_home_dir,
    };
    eprintln!(
        "agent_kind={:?} persistent_home={} (persistent across restarts)",
        agent_kind,
        persistent_home.display()
    );
    if agent_kind == AgentKind::Claude {
        eprintln!(
            "claude_home_dir={} (persistent across restarts)",
            claude_home_dir.display()
        );
    } else {
        eprintln!(
            "cursor_home_dir={} (persistent across restarts)",
            cursor_home_dir.display()
        );
    }

    // `--codebase-mode`/`--remote-codebase` on the CLI win; otherwise fall back to config.
    let codebase_mode = args.codebase_mode.or(cfg.codebase_mode);
    let managed_codebase = resolve_codebase_mode(codebase_mode.as_deref(), args.remote_codebase)
        .map_err(|e| anyhow::anyhow!(e))?;
    if managed_codebase {
        eprintln!(
            "codebase_mode=managed: repo not mounted; Claude reaches it only via mcp__tddy-tools__* calls"
        );
    }

    let agents_dir = args
        .agents_dir
        .or(cfg.agents_dir)
        .unwrap_or_else(|| session_base.join("agents"));
    // Named agents come from the CLI flag and the config list; inline defs come from the config.
    let mut named_agents = args.specialized_agent;
    named_agents.extend(cfg.specialized_agents);
    let specialized_defs =
        config::resolve_session_agents(&named_agents, &cfg.subagents, &agents_dir)?;
    if !specialized_defs.is_empty() {
        eprintln!(
            "specialized_agents={}",
            specialized_defs
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
    }

    // Config `claude_args` first, then CLI `-- <args>` — a trailing positional prompt lands last.
    let mut claude_args = cfg.claude_args;
    claude_args.extend(args.claude_args);

    let model = args
        .model
        .or(cfg.model)
        .unwrap_or_else(|| "claude-opus-4-8".to_string());
    let permission_mode = args
        .permission_mode
        .or(cfg.permission_mode)
        .unwrap_or_else(|| "auto".to_string());

    // Captured before `model`/`claude_home_dir`/`agent_kind` move into `SpawnParams` — used to
    // build the end-of-session token summary once the terminal bridge returns.
    let model_for_summary = model.clone();
    let claude_home_for_summary = claude_home_dir.clone();
    let is_claude_agent = agent_kind == AgentKind::Claude;

    let spawned = tokio::select! {
        res = spawn_claude_sandbox(SpawnParams {
            agent_kind,
            repo: args.repo,
            session_id: session_id.clone(),
            model,
            permission_mode,
            claude_binary: args.claude_binary.or(cfg.claude_binary),
            cursor_binary: args.cursor_binary.or(cfg.cursor_binary),
            tddy_tools_path: args.tddy_tools_path.or(cfg.tddy_tools_path),
            sandbox_runner_path: args.sandbox_runner_path.or(cfg.sandbox_runner_path),
            session_dir: session_dir.clone(),
            cwd: args.cwd.or(cfg.cwd),
            claude_home_dir,
            cursor_home_dir,
            remote_codebase: managed_codebase,
            specialized_defs,
            claude_args,
            mcp_log_level: args.mcp_log_level.or(cfg.mcp_log_level),
        }) => res?,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("interrupted");
            std::process::exit(130);
        }
    };

    let task_registry = TaskRegistry::new();

    // Watch the spawned sandbox / in-jail Claude process: when it exits, flip `main_process_exited`
    // so the terminal bridge stops and the sandbox never lingers after the process it exists to
    // proxy is gone. The child is shared (via a Mutex) with the post-bridge reap below so both the
    // watcher's `try_wait` and the cleanup's `kill`/`wait` can reach it.
    let child = Arc::new(Mutex::new(spawned.handle.into_child()));
    let main_process_exited = Arc::new(AtomicBool::new(false));
    let watch_stop = Arc::new(AtomicBool::new(false));
    let watcher = std::thread::spawn({
        let child = Arc::clone(&child);
        let main_process_exited = Arc::clone(&main_process_exited);
        let watch_stop = Arc::clone(&watch_stop);
        move || loop {
            if watch_stop.load(Ordering::Relaxed) {
                break;
            }
            if matches!(child.lock().unwrap().try_wait(), Ok(Some(_))) {
                main_process_exited.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    let bridge_result = bridge::run_terminal_bridge(
        &spawned.ready_marker,
        &spawned.session_id,
        &spawned.worktree_path,
        task_registry,
        Arc::clone(&main_process_exited),
    )
    .await;

    // Bridge is done — stop the watcher, then reap the child. `kill` is a no-op (logged) if the
    // process already exited on its own.
    watch_stop.store(true, Ordering::Relaxed);
    let _ = watcher.join();

    log::info!(target: "tddy_sandbox_app", "stopping sandbox session {session_id}");
    {
        let mut child = child.lock().unwrap();
        if let Err(e) = child.kill() {
            log::warn!(target: "tddy_sandbox_app", "kill sandbox child: {e}");
        }
        let _ = child.wait();
    }

    print_token_summary(
        &session_dir,
        &session_id,
        &claude_home_for_summary,
        &model_for_summary,
        is_claude_agent,
    );

    if let Err(e) = bridge_result {
        spawn::log_spawn_diagnostics(&spawned.egress_dir, &spawned.session_dir);
        return Err(e);
    }

    Ok(())
}

/// Print the per-conversation token breakdown for the finished session to stderr.
///
/// Merges the subagent conversation accounting the in-jail MCP server wrote to
/// `<session_dir>/egress/accounting.json` with the main agent's own usage (summed from its
/// transcript via [`tddy_core::token_accounting::read_main_agent_usage`]). Best-effort: a missing
/// or unreadable accounting file simply contributes no subagent rows.
fn print_token_summary(
    session_dir: &std::path::Path,
    session_id: &str,
    claude_home_dir: &std::path::Path,
    model: &str,
    include_main_agent: bool,
) {
    use tddy_core::token_accounting::{
        format_token_summary, read_main_agent_usage, ConversationRecord,
    };

    #[derive(serde::Deserialize)]
    struct AccountingFile {
        #[serde(default)]
        conversations: Vec<ConversationRecord>,
    }

    let mut records = Vec::new();
    if include_main_agent {
        records.push(read_main_agent_usage(claude_home_dir, session_id, model));
    }

    let accounting_path = session_dir.join("egress").join("accounting.json");
    if let Ok(text) = std::fs::read_to_string(&accounting_path) {
        if let Ok(parsed) = serde_json::from_str::<AccountingFile>(&text) {
            records.extend(parsed.conversations);
        }
    }

    eprintln!("{}", format_token_summary(session_id, &records));
}
