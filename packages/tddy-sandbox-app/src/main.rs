//! Standalone terminal app: spawn darwin sandbox + Claude, attach via SessionChannel gRPC.
//!
//! No host `tddy-daemon` is required. The host process:
//! 1. Spawns `sandbox-exec` → `tddy-sandbox-runner` (in-jail gRPC + Claude PTY + tddy-tools MCP)
//! 2. Dials the sandbox `SessionChannel` on loopback
//! 3. Proxies your terminal stdin/stdout and relays tool calls + HTTP egress on the host
//!
//! ```bash
//! tddy-sandbox-app --repo /path/to/git/checkout --model opus
//! ```

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
#[cfg(target_os = "linux")]
use tddy_sandbox_app::codebase_mode::managed_codebase_for_daemon_path;
#[cfg(target_os = "macos")]
use tddy_sandbox_app::codebase_mode::CodebaseMode;
use tddy_sandbox_app::codebase_mode::{refuse_unservable_codebase_home_dir, resolve_codebase_mode};
use tddy_sandbox_app::config;
#[cfg(target_os = "linux")]
use tddy_sandbox_app::daemon_client;
#[cfg(target_os = "macos")]
use tddy_sandbox_app::sandboxed_session::{
    provision, repo_build_home, ProvisioningInterrupted, SandboxedCodebaseParams,
};
#[cfg(target_os = "macos")]
use tddy_sandbox_app::{bridge, host_agent, spawn};

#[cfg(target_os = "macos")]
use spawn::{spawn_claude_sandbox, AgentKind, SpawnParams};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use tddy_core::output::SESSIONS_SUBDIR;
#[cfg(target_os = "macos")]
use tddy_task::TaskRegistry;
#[cfg(target_os = "macos")]
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
    /// Inline `subagents:` defs let you e.g. point an explorer agent at a local Ollama server
    /// without a separate agents dir.
    #[arg(long, short = 'c')]
    config: Option<PathBuf>,

    /// Base directory for session metadata (default: `$HOME/.tddy`).
    #[arg(long, env = "TDDY_SESSION_BASE")]
    session_base: Option<PathBuf>,

    /// Agent kind: `claude` (default) or `cursor` (`agent` binary).
    #[arg(long, default_value = "claude")]
    agent_kind: String,

    /// Claude model passed to the in-jail `claude` binary (default: the versionless `opus` alias).
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

    /// `--codebase-mode sandboxed` only: the jail's `$HOME`, mounted read-write and reused across
    /// sessions. This is the **build home** — `~/.cargo`, `~/.bun` and every other dependency
    /// cache the confined build fills — so keeping it out of the session tree is what stops each
    /// session refetching them through the CONNECT relay.
    /// Default: `$HOME/.tddy/sandbox-codebase-home`.
    ///
    /// "Codebase home" and "build home" are one thing under two names: the flag, the config key
    /// and the banner say *codebase* home, because what an operator picks it for is a codebase;
    /// the code, the tests and the docs below say *build* home, because what fills it is a build.
    /// Only the flag's spelling is fixed — everything internal reads "build home".
    ///
    /// This names the *base*: the session's own home is a per-repository directory under it (see
    /// `sandboxed_session::repo_build_home`), so a build cannot leave anything behind for another
    /// repository's build to pick up. Within one repository it is deliberately shared across
    /// sessions — the same arrangement a developer's real `~/.cargo` already has when two builds
    /// run at once.
    ///
    /// Refused in any other mode rather than accepted and dropped: no other placement runs a build
    /// inside the jail, so there is no home for this to be the home of.
    #[arg(long, env = "TDDY_SANDBOX_CODEBASE_HOME")]
    codebase_home_dir: Option<PathBuf>,

    /// Remote-codebase mode: don't mount `--repo` into the jail. Claude sees only the
    /// (read-only) context dir and the persistent home; the real repo is reachable only via
    /// `mcp__tddy-tools__*` calls, which the host relays against the real `--repo` path. Matches
    /// the daemon's sandboxed-session isolation model (see docs/ft/daemon/remote-codebase-mode.md).
    /// Deprecated: prefer `--codebase-mode managed`, which this remains a working alias for.
    #[arg(long)]
    remote_codebase: bool,

    /// Codebase mode: `mounted` (default) mounts `--repo` read-write into the jail; `managed`
    /// keeps the repo unmounted, reaching it only via `mcp__tddy-tools__*` calls relayed by the
    /// host. `sandboxed` (macOS) inverts the placement — the checkout and its build run inside the
    /// jail and Claude runs on this host, reaching the checkout only via `mcp__tddy-tools__*` calls
    /// dispatched *into* it (see docs/ft/coder/sandboxed-codebase-mode.md). Supersedes
    /// `--remote-codebase` (still accepted as a working alias for `managed`).
    #[arg(long)]
    codebase_mode: Option<String>,

    /// Specialized agent to wire into the session, by def name, repeatable for multiple
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

    /// Linux only: path to the running tddy-daemon's Unix socket. The Linux path talks to the
    /// daemon over gRPC instead of spawning the jail in-process. Default: resolves like the daemon
    /// itself — `${XDG_RUNTIME_DIR}/tddy-daemon.sock`, else `/run/tddy-daemon.sock`.
    #[arg(long)]
    daemon_socket: Option<PathBuf>,

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

#[cfg(target_os = "macos")]
fn default_session_base() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".tddy")
}

#[cfg(target_os = "macos")]
fn default_claude_home_dir() -> PathBuf {
    default_session_base().join("sandbox-claude-home")
}

#[cfg(target_os = "macos")]
fn default_cursor_home_dir() -> PathBuf {
    default_session_base().join("sandbox-cursor-home")
}

/// Where a `sandboxed` session's build keeps its `$HOME` when nothing named one: beside the
/// persistent agent homes under `~/.tddy`, and outside the `sessions/` tree that is discarded with
/// the session it belongs to.
#[cfg(target_os = "macos")]
fn default_codebase_home_dir() -> PathBuf {
    default_session_base().join("sandbox-codebase-home")
}

/// The startup banner's agent-home lines: which persistent `$HOME` this session's agent runs from.
///
/// Empty for `sandboxed`, and that omission is the honest answer rather than a missing one. Those
/// homes belong to an agent *inside* a jail; a `sandboxed` session's agent is an ordinary host
/// process using the host's own `~/.claude`, and the jail home named here would be neither created
/// nor mounted. Printing it would tell an operator their credentials were confined when they are
/// the one thing in the session that is not.
#[cfg(target_os = "macos")]
fn agent_home_banner(
    mode: CodebaseMode,
    agent_kind: AgentKind,
    claude_home_dir: &std::path::Path,
    cursor_home_dir: &std::path::Path,
) -> Vec<String> {
    if mode == CodebaseMode::Sandboxed {
        return Vec::new();
    }
    let (home, flag) = match agent_kind {
        AgentKind::Claude => (claude_home_dir, "claude_home_dir"),
        AgentKind::Cursor => (cursor_home_dir, "cursor_home_dir"),
    };
    vec![
        format!(
            "agent_kind={agent_kind:?} persistent_home={} (persistent across restarts)",
            home.display()
        ),
        format!("{flag}={} (persistent across restarts)", home.display()),
    ]
}

/// Repoint `<session-base>/sessions/latest` at `<session_id>` (best-effort; failures are ignored —
/// it's a convenience pointer for finding the current session's logs, never load-bearing).
#[cfg(target_os = "macos")]
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

    // macOS spawns the Seatbelt jail in-process; Linux drives a running tddy-daemon over gRPC.
    #[cfg(target_os = "macos")]
    {
        run_macos(args, cfg).await
    }
    #[cfg(target_os = "linux")]
    {
        run_linux(args, cfg).await
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (args, cfg);
        anyhow::bail!(
            "tddy-sandbox-app supports macOS (in-process Seatbelt jail) or Linux (via a running \
             tddy-daemon); this platform is unsupported"
        )
    }
}

/// Linux: send the resolved sandbox params to a running tddy-daemon and proxy the terminal to the
/// daemon-hosted sandboxed session (see `daemon_client`). The in-process Seatbelt path is macOS-only.
#[cfg(target_os = "linux")]
async fn run_linux(args: Args, cfg: config::SandboxAppConfig) -> Result<()> {
    // `--codebase-mode`/`--remote-codebase` on the CLI win; otherwise fall back to config.
    let codebase_mode = args.codebase_mode.or(cfg.codebase_mode);
    let mode = resolve_codebase_mode(codebase_mode.as_deref(), args.remote_codebase)
        .map_err(|e| anyhow::anyhow!(e))?;
    let managed_codebase =
        managed_codebase_for_daemon_path(mode).map_err(|e| anyhow::anyhow!(e))?;
    // Resolved CLI-over-config, like every other flag here, so a `codebase_home_dir:` in the YAML
    // is caught by the same refusal that catches the flag. On this platform the mode that reads it
    // was already refused above, so this is every Linux session there is.
    let codebase_home_dir = args.codebase_home_dir.or(cfg.codebase_home_dir);
    refuse_unservable_codebase_home_dir(mode, codebase_home_dir.as_deref())
        .map_err(|e| anyhow::anyhow!(e))?;
    if managed_codebase {
        eprintln!(
            "codebase_mode=managed: repo not mounted; Claude reaches it only via mcp__tddy-tools__* calls"
        );
    }

    // Specialized-agent names come from the CLI flag and the config list. The daemon resolves each
    // name against its own `<tddyhome>/agents` (+ builtins), so inline `subagents:` defs in the app
    // config cannot be honored here — the daemon never sees them. Refuse rather than silently drop
    // a subagent the caller explicitly declared.
    if !cfg.subagents.is_empty() {
        anyhow::bail!(
            "inline `subagents:` defs are not supported on the Linux daemon path — the daemon \
             resolves specialized agents from its own <tddyhome>/agents. Use `--specialized-agent \
             <name>` / config `specialized_agents:` with agents the daemon already knows."
        );
    }
    let mut specialized_agents = args.specialized_agent;
    specialized_agents.extend(cfg.specialized_agents);

    // Config `claude_args` first, then CLI `-- <args>` — a trailing positional prompt lands last.
    let mut claude_args = cfg.claude_args;
    claude_args.extend(args.claude_args);

    let model = args
        .model
        .or(cfg.model)
        .unwrap_or_else(|| tddy_core::backend::CLAUDE_DEFAULT_MODEL.to_string());
    let permission_mode = args
        .permission_mode
        .or(cfg.permission_mode)
        .unwrap_or_else(|| "auto".to_string());

    daemon_client::run(daemon_client::DaemonClientParams {
        daemon_socket: args.daemon_socket,
        repo: args.repo,
        model,
        permission_mode,
        managed_codebase,
        claude_args,
        specialized_agents,
    })
    .await
}

/// macOS: spawn the Seatbelt jail + Claude in-process and attach the terminal via SessionChannel
/// gRPC on loopback — the original no-daemon flow, unchanged.
#[cfg(target_os = "macos")]
async fn run_macos(args: Args, cfg: config::SandboxAppConfig) -> Result<()> {
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

    // Resolved before the banner, because the banner depends on it: which homes a session actually
    // uses is a question about the placement, and `sandboxed` answers it differently.
    // `--codebase-mode`/`--remote-codebase` on the CLI win; otherwise fall back to config.
    let codebase_mode = args.codebase_mode.or(cfg.codebase_mode);
    let mode = resolve_codebase_mode(codebase_mode.as_deref(), args.remote_codebase)
        .map_err(|e| anyhow::anyhow!(e))?;

    for line in agent_home_banner(mode, agent_kind, &claude_home_dir, &cursor_home_dir) {
        eprintln!("{line}");
    }
    match mode {
        CodebaseMode::Managed => eprintln!(
            "codebase_mode=managed: repo not mounted; Claude reaches it only via mcp__tddy-tools__* calls"
        ),
        CodebaseMode::Sandboxed => eprintln!(
            "codebase_mode=sandboxed: the codebase and its build are confined; the agent runs on this host"
        ),
        CodebaseMode::Mounted => {}
    }

    // Named agents come from the CLI flag and the config list; inline defs come from the config.
    let mut named_agents = args.specialized_agent;
    named_agents.extend(cfg.specialized_agents);
    let inline_subagents: Vec<String> = cfg.subagents.iter().map(|def| def.name.clone()).collect();
    refuse_unservable_sandboxed_session(mode, agent_kind, &named_agents, &inline_subagents)
        .map_err(|e| anyhow::anyhow!(e))?;
    // Resolved the same way the modes that honour it resolve it (CLI over config), because a
    // working directory the config named is just as ignored as one the flag named.
    let cwd = args.cwd.or(cfg.cwd);
    refuse_unservable_cwd(mode, cwd.as_deref()).map_err(|e| anyhow::anyhow!(e))?;
    // The mirror of that refusal: `--cwd` is read by every mode but this one, and the build's
    // `$HOME` by this mode alone. Resolved CLI-over-config for the same reason — a path the config
    // named would be dropped just as silently as one the flag named.
    let codebase_home_base = args.codebase_home_dir.or(cfg.codebase_home_dir);
    refuse_unservable_codebase_home_dir(mode, codebase_home_base.as_deref())
        .map_err(|e| anyhow::anyhow!(e))?;

    let agents_dir = args
        .agents_dir
        .or(cfg.agents_dir)
        .unwrap_or_else(|| session_base.join("agents"));
    let specialized_defs =
        config::resolve_session_agents(&named_agents, &cfg.subagents, &agents_dir)?;

    // Every tool restriction is declared on the defs themselves (`replaces:`), and declaring it is
    // all it does: the union is withdrawn from the main agent and no def gains a role from which
    // tool name it named (docs/ft/daemon/session-agent-roster.md § Tool replacement, without
    // behaviour). Keep the replaced set for the host-side policy checks.
    let replaced_tools =
        tddy_discovery::subagent::resolve_replaced_tools_for_defs(&specialized_defs);
    if !replaced_tools.is_empty() {
        eprintln!("replaced_tools={}", replaced_tools.join(","));
    }
    if !specialized_defs.is_empty() {
        eprintln!(
            "specialized_agents={}",
            specialized_defs
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );

        // Readiness gate: wake every specialized agent's endpoint and wait until each answers
        // before starting the in-jail agent CLI, so a cold/unreachable model surfaces here instead
        // of stalling the main agent's first subagent call. No fallback — the CLI is never spawned
        // if warm-up fails.
        eprintln!(
            "waking {} specialized agent(s) before starting {agent_kind:?} …",
            specialized_defs.len()
        );
        let warmup_options = tddy_discovery::warmup::WarmupOptions::default();
        tokio::select! {
            res = tddy_discovery::warmup::warm_up_agents(&specialized_defs, &warmup_options) => {
                if let Err(e) = res {
                    eprintln!("specialized agent warm-up failed: {e}");
                    return Err(anyhow::anyhow!(e.to_string()));
                }
            }
            _ = tokio::signal::ctrl_c() => {
                eprintln!("interrupted");
                std::process::exit(130);
            }
        }
    }

    // Config `claude_args` first, then CLI `-- <args>` — a trailing positional prompt lands last.
    let mut claude_args = cfg.claude_args;
    claude_args.extend(args.claude_args);

    let model = args
        .model
        .or(cfg.model)
        .unwrap_or_else(|| tddy_core::backend::CLAUDE_DEFAULT_MODEL.to_string());
    let permission_mode = args
        .permission_mode
        .or(cfg.permission_mode)
        .unwrap_or_else(|| "auto".to_string());

    // The inverted placement forks the flow here and never rejoins it: there is no jail to attach a
    // terminal to and no in-jail agent to proxy, so none of the machinery below applies.
    if mode == CodebaseMode::Sandboxed {
        return run_sandboxed_codebase(SandboxedCodebaseRun {
            repo: args.repo,
            session_id,
            session_dir,
            claude_binary: args.claude_binary.or(cfg.claude_binary),
            tddy_tools_path: args.tddy_tools_path.or(cfg.tddy_tools_path),
            sandbox_runner_path: args.sandbox_runner_path.or(cfg.sandbox_runner_path),
            codebase_home_base: codebase_home_base.unwrap_or_else(default_codebase_home_dir),
            model,
            permission_mode,
            claude_args,
            mcp_log_level: args.mcp_log_level.or(cfg.mcp_log_level),
        })
        .await;
    }

    // Captured before `model`/`claude_home_dir`/`agent_kind` move into `SpawnParams` — used to
    // build the end-of-session token summary once the terminal bridge returns.
    let model_for_summary = model.clone();
    let claude_home_for_summary = claude_home_dir.clone();
    let is_claude_agent = agent_kind == AgentKind::Claude;

    // Register the reusable-LSP executor before spawning the jail, so the spawn-time
    // availability check can gate the in-jail `Lsp*` tools, and relayed `Lsp*` tool calls
    // resolve to a real, reused language server. Uses its own task registry.
    {
        let lsp_registry = tddy_lsp_executor::register(
            TaskRegistry::new(),
            tddy_lsp::LspAllowList::rust_only(),
            std::time::Duration::from_secs(300),
        );
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
            ticker.tick().await; // consume the immediate first tick
            loop {
                ticker.tick().await;
                lsp_registry.reap_idle().await;
            }
        });
    }

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
            cwd,
            claude_home_dir,
            cursor_home_dir,
            codebase_mode: mode,
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
        &session_dir,
        task_registry,
        Arc::clone(&main_process_exited),
        replaced_tools,
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

/// What a `sandboxed` session cannot serve, refused before anything is spawned.
///
/// Both refusals are named in the PRD's "deliberately not in scope", and both are loud on purpose.
/// `--agent-kind cursor` keeps today's placement because Cursor's tool surface is not withdrawable
/// the way Claude's `--disallowedTools` makes Claude's — an inverted placement could not make the
/// confinement claim, so quietly keeping the old one would leave the caller confining the agent
/// under a flag that promises to confine the code. A specialized agent's roster is seeded into the
/// *in-jail* `tddy-tools --mcp` server's env, and this mode runs no MCP server inside the jail —
/// so the roster would silently not exist.
///
/// Any other mode is served exactly as it is today: this refuses combinations, never the modes.
#[cfg(target_os = "macos")]
fn refuse_unservable_sandboxed_session(
    mode: CodebaseMode,
    agent_kind: AgentKind,
    named_agents: &[String],
    inline_subagents: &[String],
) -> Result<(), String> {
    if mode != CodebaseMode::Sandboxed {
        return Ok(());
    }
    if agent_kind == AgentKind::Cursor {
        return Err(
            "--codebase-mode sandboxed cannot be combined with --agent-kind cursor: the mode \
             withdraws the agent's native filesystem and shell tools so that the jail is the only \
             route to the checkout, and Cursor's tool surface is not withdrawable the way Claude's \
             --disallowedTools makes Claude's. Use --agent-kind claude, or keep today's placement \
             with --codebase-mode mounted or managed."
                .to_string(),
        );
    }
    let requested: Vec<&str> = named_agents
        .iter()
        .chain(inline_subagents)
        .map(String::as_str)
        .collect();
    if !requested.is_empty() {
        return Err(format!(
            "--codebase-mode sandboxed cannot be combined with specialized agents (asked for: \
             {}): the roster is seeded into the in-jail `tddy-tools --mcp` server, and this mode \
             runs no MCP server inside the jail. Drop --specialized-agent / `specialized_agents:` \
             / `subagents:`, or use --codebase-mode mounted or managed.",
            requested.join(", ")
        ));
    }
    Ok(())
}

/// Refuse `--cwd` in a `sandboxed` session, where there is no in-jail agent to place.
///
/// The flag names the working directory of an agent *inside the jail*, and every other mode honours
/// it there. This mode's jail holds a tool engine and a build; its agent is a host process whose
/// working directory is the checkout, because that is what makes it an agent for this project.
/// Neither meaning is the one the caller asked for, so the session says so instead of starting and
/// leaving them to believe the agent moved.
#[cfg(target_os = "macos")]
fn refuse_unservable_cwd(mode: CodebaseMode, cwd: Option<&std::path::Path>) -> Result<(), String> {
    match (mode, cwd) {
        (CodebaseMode::Sandboxed, Some(cwd)) => Err(format!(
            "--codebase-mode sandboxed cannot be combined with --cwd ({}): the flag places an \
             agent inside the jail, and this mode's jail has no agent in it — the agent runs on \
             this host, with the checkout as its working directory. Drop --cwd, or use \
             --codebase-mode mounted or managed.",
            cwd.display()
        )),
        _ => Ok(()),
    }
}

/// A `sandboxed` session, as `run_macos` resolved it.
#[cfg(target_os = "macos")]
struct SandboxedCodebaseRun {
    repo: PathBuf,
    session_id: String,
    session_dir: PathBuf,
    claude_binary: Option<String>,
    tddy_tools_path: Option<String>,
    sandbox_runner_path: Option<String>,
    /// Where this host keeps its build homes (the `--codebase-home-dir` base). The session's own
    /// is a per-repository directory under it — see [`repo_build_home`].
    codebase_home_base: PathBuf,
    model: String,
    permission_mode: String,
    claude_args: Vec<String>,
    mcp_log_level: Option<String>,
}

/// What the jail is provisioned with, from what the command line resolved.
///
/// A named function rather than a literal inside [`run_sandboxed_codebase`], because one of these
/// fields is a security boundary and the wrong value for it type-checks.
/// [`SandboxedCodebaseParams::repo_build_home`] is *this repository's* `$HOME`;
/// [`SandboxedCodebaseRun::codebase_home_base`] is the base holding every repository's, and it is
/// a `PathBuf` that fits the field just as well. Handing the base over would give every checkout
/// on the host one shared build home — the cross-repo poisoning channel the keying closes — and
/// would do it silently, since a shared home works. Resolving it in one place gives that step a
/// name a test can hold to.
#[cfg(target_os = "macos")]
fn sandboxed_codebase_params(
    run: &SandboxedCodebaseRun,
    canonical_repo: PathBuf,
    tddy_tools_path: String,
) -> SandboxedCodebaseParams {
    let build_home = repo_build_home(&run.codebase_home_base, &canonical_repo);
    SandboxedCodebaseParams {
        repo: canonical_repo,
        session_id: run.session_id.clone(),
        session_dir: run.session_dir.clone(),
        sandbox_runner_path: run.sandbox_runner_path.clone(),
        tddy_tools_path: Some(tddy_tools_path),
        repo_build_home: build_home,
    }
}

/// macOS, `--codebase-mode sandboxed`: confine the checkout and run the agent here.
///
/// The inversion of [`run_macos`]'s other two paths, and much shorter than either, because almost
/// everything they do is about an agent that is behind a jail. This one is not: `claude` is an
/// ordinary child of this process with inherited stdin/stdout/stderr, so it holds the real
/// controlling terminal and Ctrl-C, window resize and `$TERM` are the terminal's own business. No
/// PTY, no terminal bridge, no OSC resize convention — the only thing between the agent and the
/// checkout is the tool socket.
#[cfg(target_os = "macos")]
async fn run_sandboxed_codebase(run: SandboxedCodebaseRun) -> Result<()> {
    use anyhow::Context;

    // Resolved before the jail is built: a session whose agent binary cannot be found should fail
    // while there is still no `sandbox-exec` child to clean up.
    let claude_binary = spawn::resolve_claude_binary(run.claude_binary.as_deref())
        .map_err(|e| anyhow::anyhow!(e))?;
    // Resolved once, for both ends: the jail is told which `tddy-tools` it holds, and the agent's
    // MCP server on this host runs that same one. A session pointed at a specific build must not
    // get it on one side of the boundary and the default sibling on the other.
    let tddy_tools_path =
        tddy_daemon::sandbox_session::resolve_tddy_tools_path(run.tddy_tools_path.as_deref());

    // The build home is keyed on the *canonical* checkout, so a repo reached by two spellings —
    // a symlink, a trailing slash — is one repository with one home rather than two.
    let repo = run
        .repo
        .canonicalize()
        .with_context(|| format!("canonicalize repo {}", run.repo.display()))?;
    let params = sandboxed_codebase_params(&run, repo, tddy_tools_path.clone());
    eprintln!(
        "codebase_home_dir={} (the jail's $HOME for this repo, persistent across sessions)",
        params.repo_build_home.display()
    );

    // Ctrl-C is `provision`'s own business, not something to race it with from out here: it owns
    // the jail's handle, and a handle dropped mid-flight kills nothing.
    let session = match provision(params).await {
        Ok(session) => session,
        Err(e) if e.downcast_ref::<ProvisioningInterrupted>().is_some() => {
            eprintln!("interrupted");
            std::process::exit(130);
        }
        Err(e) => return Err(e),
    };

    eprintln!(
        "jail: tddy-sandbox-runner --workspace-tools {} (egress via host CONNECT relay on \
         127.0.0.1:{})",
        session.worktree().display(),
        session.egress_shim_port()
    );
    eprintln!(
        "host tools withdrawn: {}",
        tddy_sandbox_recipes::build_host_agent_disallowlist().join(", ")
    );

    let argv = host_agent::build_host_agent_argv(host_agent::HostAgentArgs {
        claude_binary,
        model: run.model.clone(),
        permission_mode: run.permission_mode,
        session_id: run.session_id.clone(),
        // The session's host-only directory, which is the one part of its tree the jail has no
        // grant over. What goes in here is the MCP config, whose `command` and `env` this host
        // executes unconfined on every reconnect — put it anywhere the jail can write and a
        // hostile build gets a shell on the host by editing a file.
        host_config_dir: session.host_dir().to_path_buf(),
        tddy_tools_path: PathBuf::from(tddy_tools_path),
        tool_ipc_socket: session.tool_ipc_socket().to_path_buf(),
        claude_args: run.claude_args,
        mcp_log_level: run.mcp_log_level,
        egress_dir: Some(session.egress_dir().to_path_buf()),
        // Specialized agents are refused in this mode, so no def took a tool away.
        replaced_tools: Vec::new(),
    })?;

    // The agent owns the terminal, so Ctrl-C is *its* interrupt: SIGINT reaches every process in
    // the foreground group, and this one has to survive it to tear the jail down afterwards.
    // Registering a handler that does nothing replaces the default disposition, which would kill
    // this process and leave the jail with nobody to reap it.
    let interrupts = tokio::spawn(async { while tokio::signal::ctrl_c().await.is_ok() {} });

    let worktree = session.worktree().to_path_buf();
    let status = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&argv[0])
            .args(&argv[1..])
            // The checkout, because an agent whose working directory is somewhere else is a
            // generic agent rather than this project's. What the directory does *not* buy the
            // repository is a way to run something here: its `.claude/settings.json` hooks and its
            // `.mcp.json` servers are both refused at the argv (`build_host_agent_argv`), and
            // acting on the checkout is what the agent cannot do except by asking the jail.
            .current_dir(&worktree)
            .status()
    })
    .await
    .context("host agent task join")?
    .context("run the host agent")?;

    interrupts.abort();
    session.stop();

    // The agent ran unconfined with the host's own `~/.claude`, so that is where its transcript is
    // — not in a jail home, which this mode has none of. With no `$HOME` there is no such tree, and
    // a summary gathered from a stand-in path prints an empty table that reads like a session which
    // spent no tokens; say why instead.
    match host_home(std::env::var_os("HOME")) {
        Ok(home) => print_token_summary(&run.session_dir, &run.session_id, &home, &run.model, true),
        Err(reason) => eprintln!("no token summary: {reason}"),
    }

    if !status.success() {
        anyhow::bail!("the host agent exited with {status}");
    }
    Ok(())
}

/// This host's `$HOME`, where an unconfined agent keeps its `~/.claude`, or why there is none.
///
/// Takes the environment's answer rather than reading it, so the "and if there isn't one?" case is
/// a value a caller can handle and a test can name.
#[cfg(target_os = "macos")]
fn host_home(env_home: Option<std::ffi::OsString>) -> Result<PathBuf, String> {
    env_home.map(PathBuf::from).ok_or_else(|| {
        "HOME is unset, so the host agent's transcript under ~/.claude cannot be located"
            .to_string()
    })
}

/// Print the per-conversation token breakdown for the finished session to stderr.
///
/// Combines three sources: the main Claude agent's own usage (from its transcript, via
/// [`tddy_core::backend::read_claude_transcript_usage`]), each of Claude's nested Task-tool
/// subagents ([`tddy_core::backend::read_claude_subagent_usages`]), and the tddy subagent
/// conversations the in-jail MCP server wrote to `<session_dir>/egress/accounting.json`.
/// Best-effort: a missing or unreadable accounting file simply contributes no tddy-subagent rows.
///
/// macOS-only: the in-process `run_macos` flow spawns and reaps Claude itself, so it is the only
/// path that can read the finished transcript. The Linux path delegates the session lifecycle to
/// the daemon (`run_linux` → `daemon_client::run`) and never reaps Claude here, so it has no
/// summary to print.
#[cfg(target_os = "macos")]
fn print_token_summary(
    session_dir: &std::path::Path,
    session_id: &str,
    claude_home_dir: &std::path::Path,
    model: &str,
    include_main_agent: bool,
) {
    use tddy_core::backend::gather_session_usage;
    use tddy_core::token_accounting::format_token_summary;

    let records = gather_session_usage(
        session_dir,
        session_id,
        claude_home_dir,
        model,
        include_main_agent,
    );

    eprintln!("{}", format_token_summary(session_id, &records));
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::path::Path;

    // ─── What a sandboxed session refuses to be combined with ───────────────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md (§ What is deliberately not in scope)
    // Changeset: docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md

    const NO_AGENTS: &[String] = &[];

    fn an_agent_named(name: &str) -> Vec<String> {
        vec![name.to_string()]
    }

    /// Cursor's tool surface is not withdrawable the way Claude's `--disallowedTools` makes
    /// Claude's, so an inverted placement could not make the confinement claim. Keeping today's
    /// placement quietly would leave the caller confining the agent under a flag that promises to
    /// confine the code.
    #[test]
    fn a_sandboxed_session_refuses_a_cursor_agent() {
        // Given / When
        let message = refuse_unservable_sandboxed_session(
            CodebaseMode::Sandboxed,
            AgentKind::Cursor,
            NO_AGENTS,
            NO_AGENTS,
        )
        .expect_err("a sandboxed session must refuse an agent whose tools it cannot withdraw");

        // Then
        assert!(
            message.contains("cursor") && message.contains("sandboxed"),
            "the refusal must name both halves of the combination it rejects; message was: \
             {message}"
        );
    }

    /// A specialized agent's roster is seeded into the *in-jail* `tddy-tools --mcp` server's env,
    /// and this mode runs no MCP server inside the jail — so the agent that was asked for would
    /// simply not exist.
    #[test]
    fn a_sandboxed_session_refuses_a_named_specialized_agent() {
        // Given / When
        let message = refuse_unservable_sandboxed_session(
            CodebaseMode::Sandboxed,
            AgentKind::Claude,
            &an_agent_named("explorer"),
            NO_AGENTS,
        )
        .expect_err("a sandboxed session must refuse a specialized agent it cannot seed");

        // Then
        assert!(
            message.contains("explorer"),
            "the refusal must name the agent that was asked for; message was: {message}"
        );
    }

    /// The same refusal for an agent declared inline in the config rather than by name: where the
    /// roster came from does not change that there is no in-jail server to seed it into.
    #[test]
    fn a_sandboxed_session_refuses_an_inline_subagent_def() {
        // Given / When
        let message = refuse_unservable_sandboxed_session(
            CodebaseMode::Sandboxed,
            AgentKind::Claude,
            NO_AGENTS,
            &an_agent_named("local-coder"),
        )
        .expect_err("a sandboxed session must refuse an inline subagent def too");

        // Then
        assert!(
            message.contains("local-coder"),
            "the refusal must name the agent that was asked for; message was: {message}"
        );
    }

    /// The combination the mode exists for: a plain Claude agent and no roster at all.
    #[test]
    fn a_sandboxed_session_with_a_plain_claude_agent_is_served() {
        // Given / When
        let served = refuse_unservable_sandboxed_session(
            CodebaseMode::Sandboxed,
            AgentKind::Claude,
            NO_AGENTS,
            NO_AGENTS,
        );

        // Then
        assert_eq!(served, Ok(()));
    }

    /// Neither refusal is about Cursor or subagents as such — both are about *this* placement. A
    /// mounted session keeps serving exactly what it serves today.
    #[test]
    fn a_mounted_session_still_serves_a_cursor_agent_with_specialized_agents() {
        // Given / When
        let served = refuse_unservable_sandboxed_session(
            CodebaseMode::Mounted,
            AgentKind::Cursor,
            &an_agent_named("explorer"),
            &an_agent_named("local-coder"),
        );

        // Then
        assert_eq!(served, Ok(()));
    }

    // ─── What a sandboxed session says about itself before it starts ────────────

    /// `--cwd` names the working directory of an agent *inside the jail*, and this mode's jail has
    /// no agent in it. Carrying the flag into the session unread would let a caller believe they
    /// had moved the agent somewhere they had not.
    #[test]
    fn a_sandboxed_session_refuses_a_working_directory_override() {
        // Given / When
        let message = refuse_unservable_cwd(
            CodebaseMode::Sandboxed,
            Some(Path::new("/Users/dev/code/app/packages/api")),
        )
        .expect_err("a sandboxed session must refuse a flag it cannot honour");

        // Then
        assert!(
            message.contains("--cwd") && message.contains("sandboxed"),
            "the refusal must name both halves of the combination it rejects; message was: \
             {message}"
        );
    }

    /// The refusal is about this placement, not about `--cwd`: the modes that put the agent in the
    /// jail keep honouring it exactly as they do today.
    #[test]
    fn a_mounted_session_still_honours_a_working_directory_override() {
        // Given / When
        let served = refuse_unservable_cwd(
            CodebaseMode::Mounted,
            Some(Path::new("/Users/dev/code/app/packages/api")),
        );

        // Then
        assert_eq!(served, Ok(()));
    }

    /// The jail homes the banner names belong to an agent *inside* a jail. A `sandboxed` session's
    /// agent runs on this host with the real `~/.claude`, and that home is never created or
    /// mounted — printing it would tell an operator their credentials were confined when they are
    /// the one thing in the session that is not.
    #[test]
    fn a_sandboxed_sessions_banner_names_no_jail_agent_home() {
        // Given / When
        let lines = agent_home_banner(
            CodebaseMode::Sandboxed,
            AgentKind::Claude,
            Path::new("/Users/dev/.tddy/sandbox-claude-home"),
            Path::new("/Users/dev/.tddy/sandbox-cursor-home"),
        );

        // Then
        assert_eq!(lines, Vec::<String>::new());
    }

    /// …and the modes whose agent really does live in that home still say so.
    #[test]
    fn a_mounted_sessions_banner_names_the_jail_home_its_agent_runs_from() {
        // Given / When
        let lines = agent_home_banner(
            CodebaseMode::Mounted,
            AgentKind::Claude,
            Path::new("/Users/dev/.tddy/sandbox-claude-home"),
            Path::new("/Users/dev/.tddy/sandbox-cursor-home"),
        );

        // Then
        assert!(
            lines
                .iter()
                .any(|line| line.contains("/Users/dev/.tddy/sandbox-claude-home")),
            "a mounted session's agent runs from the jail home and the banner must say so; \
             lines were: {lines:?}"
        );
    }

    /// The token summary is read out of the agent's transcript under `~/.claude`. With no `$HOME`
    /// there is no such tree, and a summary computed against `/` would print an empty table that
    /// reads like a session which used no tokens.
    #[test]
    fn a_missing_home_is_reported_rather_than_summarised_against_the_filesystem_root() {
        // Given / When
        let refused = host_home(None)
            .expect_err("a host with no $HOME has nowhere to read the agent's transcript from");

        // Then
        assert!(
            refused.contains("HOME"),
            "the reason must name what is missing; message was: {refused}"
        );
    }

    // ─── Where a sandboxed session's build keeps its home ───────────────────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md
    // Changeset: docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md

    /// The build's `$HOME` holds the dependency caches every `sandboxed` session on this host
    /// refills — so by default it sits directly under `~/.tddy`, beside the persistent agent homes
    /// and outside `sessions/`, which is the tree a finished session takes with it.
    #[test]
    fn the_default_build_home_sits_directly_under_the_hosts_tddy_dir() {
        // Given
        let tddy_dir = default_session_base();

        // When
        let build_home = default_codebase_home_dir();

        // Then
        assert_eq!(
            build_home.parent(),
            Some(tddy_dir.as_path()),
            "the default build home must be a direct child of {}, not nested in a session; it was \
             {}",
            tddy_dir.display(),
            build_home.display()
        );
    }

    /// The helper above is only half the protection: the base is a `PathBuf` and so is the home
    /// keyed out of it, so a session wired to pass `codebase_home_base` straight to
    /// [`SandboxedCodebaseParams::repo_build_home`] compiles and runs — and every checkout on the
    /// host is back to one shared `$HOME`. So this asks the question of the wiring: two sessions,
    /// two repositories, one base, as an operator's second `tddy-sandbox-app` invocation of the
    /// day produces them.
    #[test]
    fn two_sessions_against_different_repositories_are_provisioned_with_different_build_homes() {
        // Given — one host, one build-home base, two checkouts
        let base = PathBuf::from("/Users/dev/.tddy/sandbox-codebase-home");

        // When
        let audited = sandboxed_codebase_params(
            &a_sandboxed_run(&base),
            PathBuf::from("/Users/dev/code/my-app"),
            "/opt/tddy/tddy-tools".to_string(),
        );
        let unaudited = sandboxed_codebase_params(
            &a_sandboxed_run(&base),
            PathBuf::from("/Users/dev/code/a-cloned-fork"),
            "/opt/tddy/tddy-tools".to_string(),
        );

        // Then
        assert_ne!(
            audited.repo_build_home, unaudited.repo_build_home,
            "two repositories provisioned from one base must not be handed the same jail $HOME"
        );
    }

    /// …and neither of them is the base itself, which is the shape the mistake takes: a base
    /// mounted as one jail's `$HOME` is every other repository's home mounted with it.
    #[test]
    fn a_sessions_build_home_is_never_the_base_every_repository_shares() {
        // Given
        let base = PathBuf::from("/Users/dev/.tddy/sandbox-codebase-home");

        // When
        let params = sandboxed_codebase_params(
            &a_sandboxed_run(&base),
            PathBuf::from("/Users/dev/code/my-app"),
            "/opt/tddy/tddy-tools".to_string(),
        );

        // Then
        assert_eq!(
            params.repo_build_home.parent(),
            Some(base.as_path()),
            "the jail's $HOME must be a directory under the base, not the base; it was {}",
            params.repo_build_home.display()
        );
    }

    /// A `sandboxed` run as the command line resolves one, with everything but the build-home base
    /// left at whatever a session that never starts would carry.
    fn a_sandboxed_run(codebase_home_base: &Path) -> SandboxedCodebaseRun {
        SandboxedCodebaseRun {
            repo: PathBuf::from("/Users/dev/code/my-app"),
            session_id: "019d1e2f-3456-7890-abcd-ef0123456789".to_string(),
            session_dir: PathBuf::from("/Users/dev/.tddy/sessions/019d1e2f"),
            claude_binary: None,
            tddy_tools_path: None,
            sandbox_runner_path: None,
            codebase_home_base: codebase_home_base.to_path_buf(),
            model: "opus".to_string(),
            permission_mode: "auto".to_string(),
            claude_args: vec![],
            mcp_log_level: None,
        }
    }
}
