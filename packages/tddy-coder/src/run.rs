//! Run logic shared by tddy-coder and tddy-demo binaries.
//!
//! Args is the common runtime type. CoderArgs and DemoArgs are CLI parser types
//! with different agent constraints; both convert to Args via From.

use anyhow::Context;
use clap::Parser;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::{
    get_session_for_tag, next_goal_for_state, parse_acceptance_tests_response,
    parse_evaluate_response, parse_green_response, parse_red_response, parse_refactor_response,
    parse_update_docs_response, parse_validate_subagents_response, read_changeset, AnyBackend,
    ClaudeCodeBackend, CursorBackend, ProgressEvent, SharedBackend, StubBackend, WorkflowEngine,
};

use crate::plain;
use crate::tty::should_run_tui;
use tddy_core::{find_git_root, Presenter};

use crate::disable_raw_mode;

/// Reject if `output_dir` resolves to the git repo root (would pollute the repo).
fn reject_output_dir_if_repo_root(output_dir: &Path) -> anyhow::Result<()> {
    let resolved = output_dir
        .canonicalize()
        .or_else(|_| std::fs::canonicalize(output_dir.parent().unwrap_or(output_dir)))
        .context("resolve output-dir path")?;
    let cwd = std::env::current_dir().context("current dir")?;
    let git_root = find_git_root(&cwd);
    let git_root_resolved = git_root.canonicalize().unwrap_or_else(|_| git_root.clone());
    if resolved == git_root_resolved {
        anyhow::bail!(
            "--output-dir must not be the repository root ({}); use a subdirectory or omit for $HOME/.tddy/sessions",
            output_dir.display()
        );
    }
    Ok(())
}

/// Shared main entry: panic hook, Ctrl+C handler, run_with_args, exit logic.
/// Use from both tddy-coder and tddy-demo binaries.
pub fn run_main(mut args: Args) {
    args.session_id = Some(uuid::Uuid::now_v7().to_string());
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show,
        );
        let _ = disable_raw_mode();
        original_hook(info);
    }));

    let shutdown = Arc::new(AtomicBool::new(false));
    let ctrlc_pressed = Arc::new(AtomicBool::new(false));
    let shutdown_for_handler = shutdown.clone();
    let ctrlc_pressed_for_handler = ctrlc_pressed.clone();

    ctrlc::set_handler(move || {
        tddy_core::kill_child_process();
        ctrlc_pressed_for_handler.store(true, Ordering::Relaxed);
        shutdown_for_handler.store(true, Ordering::Relaxed);
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show,
        );
        let _ = disable_raw_mode();
        let _ = std::io::stderr().flush();
    })
    .expect("failed to set Ctrl+C handler");

    match run_with_args(&args, shutdown) {
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        Ok(()) => {
            if ctrlc_pressed.load(Ordering::Relaxed) {
                std::process::exit(130);
            }
        }
    }
}

/// Common runtime args. Use CoderArgs or DemoArgs for CLI parsing, then convert via From.
/// session_id is set at program start (in run_main) and used across the workflow.
#[derive(Debug, Clone)]
pub struct Args {
    pub goal: Option<String>,
    /// None = use $HOME/.tddy/sessions. Some(path) = explicit; must not be repo root.
    pub output_dir: Option<PathBuf>,
    pub plan_dir: Option<PathBuf>,
    pub conversation_output: Option<PathBuf>,
    pub model: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub debug: bool,
    pub debug_output: Option<PathBuf>,
    pub agent: String,
    pub prompt: Option<String>,
    /// When Some(port), gRPC server runs alongside TUI on the given port.
    pub grpc: Option<u16>,
    /// Session ID set at program start; used for exit output when no plan_dir.
    pub session_id: Option<String>,
    /// When true, run as headless gRPC daemon (no TUI).
    pub daemon: bool,
    /// LiveKit server WebSocket URL (e.g. ws://localhost:7880)
    pub livekit_url: Option<String>,
    /// LiveKit access token for room join
    pub livekit_token: Option<String>,
    /// LiveKit room name
    pub livekit_room: Option<String>,
    /// LiveKit participant identity
    pub livekit_identity: Option<String>,
}

/// CLI args for tddy-coder binary: agent is claude or cursor.
#[derive(Parser, Debug, Clone)]
#[command(name = "tddy-coder")]
#[command(about = "TDD-driven coder for PRD-based development workflow")]
pub struct CoderArgs {
    /// Goal to execute: plan, acceptance-tests, red, green, demo, evaluate, validate, refactor. Omit to run full workflow.
    #[arg(long, value_parser = ["plan", "acceptance-tests", "red", "green", "demo", "evaluate", "validate", "refactor", "update-docs"])]
    pub goal: Option<String>,

    /// Output directory for planning artifacts. Omit to use $HOME/.tddy/sessions. Must not be the repo root.
    #[arg(long)]
    pub output_dir: Option<PathBuf>,

    /// Plan directory (required when goal is acceptance-tests, red, green, demo, validate, refactor, or update-docs)
    #[arg(long)]
    pub plan_dir: Option<PathBuf>,

    /// Write entire agent conversation (raw bytes) to file
    #[arg(long)]
    pub conversation_output: Option<PathBuf>,

    /// Model name for Claude Code CLI (e.g. sonnet)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Extra tools to add to the goal's allowlist (comma-separated, e.g. "Bash(npm install)")
    #[arg(long, value_delimiter = ',')]
    pub allowed_tools: Option<Vec<String>>,

    /// Enable debug logging (stderr in plain mode, TUI debug area in TUI mode)
    #[arg(long)]
    pub debug: bool,

    /// Enable debug logging and redirect to file (avoids stderr/TUI corruption)
    #[arg(long)]
    pub debug_output: Option<PathBuf>,

    /// Agent backend: claude or cursor (default: claude)
    #[arg(long, default_value = "claude", value_parser = ["claude", "cursor"])]
    pub agent: String,

    /// Feature description (alternative to stdin). When set, skips interactive/piped input.
    #[arg(long)]
    pub prompt: Option<String>,

    /// Start gRPC server alongside TUI for programmatic remote control (e.g. --grpc 50052)
    #[arg(long, value_name = "PORT", default_missing_value = "50051")]
    pub grpc: Option<u16>,

    /// Run as headless gRPC daemon (systemd-friendly)
    #[arg(long)]
    pub daemon: bool,

    /// LiveKit server WebSocket URL (e.g. ws://localhost:7880). Requires --livekit-token, --livekit-room, --livekit-identity.
    #[arg(long)]
    pub livekit_url: Option<String>,

    /// LiveKit access token for room join
    #[arg(long)]
    pub livekit_token: Option<String>,

    /// LiveKit room name
    #[arg(long)]
    pub livekit_room: Option<String>,

    /// LiveKit participant identity
    #[arg(long)]
    pub livekit_identity: Option<String>,
}

/// CLI args for tddy-demo binary: agent is stub only.
#[derive(Parser, Debug, Clone)]
#[command(name = "tddy-demo")]
#[command(about = "Same app as tddy-coder with StubBackend (identical TUI, CLI, workflow)")]
pub struct DemoArgs {
    /// Goal to execute: plan, acceptance-tests, red, green, demo, evaluate, validate, refactor. Omit to run full workflow.
    #[arg(long, value_parser = ["plan", "acceptance-tests", "red", "green", "demo", "evaluate", "validate", "refactor", "update-docs"])]
    pub goal: Option<String>,

    /// Output directory for planning artifacts. Omit to use $HOME/.tddy/sessions. Must not be the repo root.
    #[arg(long)]
    pub output_dir: Option<PathBuf>,

    /// Plan directory (required when goal is acceptance-tests, red, green, demo, validate, refactor, or update-docs)
    #[arg(long)]
    pub plan_dir: Option<PathBuf>,

    /// Write entire agent conversation (raw bytes) to file
    #[arg(long)]
    pub conversation_output: Option<PathBuf>,

    /// Model name for Claude Code CLI (e.g. sonnet)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Extra tools to add to the goal's allowlist (comma-separated, e.g. "Bash(npm install)")
    #[arg(long, value_delimiter = ',')]
    pub allowed_tools: Option<Vec<String>>,

    /// Enable debug logging (stderr in plain mode, TUI debug area in TUI mode)
    #[arg(long)]
    pub debug: bool,

    /// Enable debug logging and redirect to file (avoids stderr/TUI corruption)
    #[arg(long)]
    pub debug_output: Option<PathBuf>,

    /// Agent backend: stub only (default: stub)
    #[arg(long, default_value = "stub", value_parser = ["stub"])]
    pub agent: String,

    /// Feature description (alternative to stdin). When set, skips interactive/piped input.
    #[arg(long)]
    pub prompt: Option<String>,

    /// Start gRPC server alongside TUI for programmatic remote control (e.g. --grpc 50052)
    #[arg(long, value_name = "PORT", default_missing_value = "50051")]
    pub grpc: Option<u16>,

    /// Run as headless gRPC daemon (systemd-friendly)
    #[arg(long)]
    pub daemon: bool,
}

impl From<CoderArgs> for Args {
    fn from(a: CoderArgs) -> Args {
        Args {
            goal: a.goal,
            output_dir: a.output_dir,
            plan_dir: a.plan_dir,
            conversation_output: a.conversation_output,
            model: a.model,
            allowed_tools: a.allowed_tools,
            debug: a.debug,
            debug_output: a.debug_output,
            agent: a.agent,
            prompt: a.prompt,
            grpc: a.grpc,
            session_id: None,
            daemon: a.daemon,
            livekit_url: a.livekit_url,
            livekit_token: a.livekit_token,
            livekit_room: a.livekit_room,
            livekit_identity: a.livekit_identity,
        }
    }
}

impl From<DemoArgs> for Args {
    fn from(a: DemoArgs) -> Args {
        Args {
            goal: a.goal,
            output_dir: a.output_dir,
            plan_dir: a.plan_dir,
            conversation_output: a.conversation_output,
            model: a.model,
            allowed_tools: a.allowed_tools,
            debug: a.debug,
            debug_output: a.debug_output,
            agent: a.agent,
            prompt: a.prompt,
            grpc: a.grpc,
            session_id: None,
            daemon: a.daemon,
            livekit_url: None,
            livekit_token: None,
            livekit_room: None,
            livekit_identity: None,
        }
    }
}

/// Main entry point. Run the workflow with the given args.
pub fn run_with_args(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    if args.daemon {
        return run_daemon(args, shutdown);
    }
    if args.goal.is_none() {
        let use_tui = should_run_tui(io::stdin().is_terminal(), io::stderr().is_terminal());
        if use_tui {
            return run_full_workflow_tui(args, shutdown);
        }
        return run_full_workflow_plain(args, shutdown);
    }

    log::debug!(
        "[tddy-coder] goal: {}, agent: {}, model: {}",
        args.goal.as_deref().unwrap_or("(none)"),
        args.agent,
        args.model.as_deref().unwrap_or("(default)")
    );

    let backend = create_backend(&args.agent, None, None);

    if args.goal.as_deref() == Some("acceptance-tests") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for acceptance-tests goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, |_| {});
        return run_goal_plain(args, backend, "acceptance-tests", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("green") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for green goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, |c| {
            c.insert("run_demo".to_string(), serde_json::json!(false));
        });
        return run_goal_plain(args, backend, "green", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("evaluate") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for evaluate"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, |_| {});
        return run_goal_plain(args, backend, "evaluate", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("demo") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for demo goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, |_| {});
        return run_goal_plain(args, backend, "demo", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("red") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for red goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, |_| {});
        return run_goal_plain(args, backend, "red", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("validate") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for validate goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, |_| {});
        return run_goal_plain(args, backend, "validate", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("refactor") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for refactor goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, |_| {});
        return run_goal_plain(args, backend, "refactor", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("update-docs") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for update-docs goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, |_| {});
        return run_goal_plain(args, backend, "update-docs", ctx, true, &shutdown);
    }

    if args.goal.as_deref() != Some("plan") {
        anyhow::bail!(
            "unsupported goal: {}",
            args.goal.as_deref().unwrap_or("(none)")
        );
    }

    let input = read_feature_input(args).context("read feature description")?;
    let input = input.trim().to_string();
    if input.is_empty() {
        anyhow::bail!("empty feature description");
    }

    let (plan_dir, output_dir_for_ctx) = if let Some(output_dir) = &args.output_dir {
        reject_output_dir_if_repo_root(output_dir)?;
        let plan_dir = output_dir.join(tddy_core::output::slugify_directory_name(&input));
        std::fs::create_dir_all(&plan_dir).context("create plan directory")?;
        (plan_dir, output_dir.clone())
    } else {
        #[cfg(unix)]
        {
            let home = std::env::var("HOME").map_err(|_| {
                anyhow::anyhow!("HOME not set; cannot create session under ~/.tddy")
            })?;
            let base = PathBuf::from(&home).join(".tddy");
            let plan_dir =
                tddy_core::output::create_session_dir_in(&base).context("create session dir")?;
            let output_dir_for_ctx =
                std::env::current_dir().context("current dir for agent working_dir")?;
            (plan_dir, output_dir_for_ctx)
        }
        #[cfg(not(unix))]
        {
            anyhow::bail!(
                "plan without --output-dir requires HOME (Unix) or USERPROFILE (Windows); \
                 use --output-dir <path> explicitly"
            );
        }
    };

    let conv = resolve_log_defaults(args, &plan_dir);
    let ctx = build_goal_context(args, None, &conv, |c| {
        c.insert("feature_input".to_string(), serde_json::json!(input));
        c.insert(
            "output_dir".to_string(),
            serde_json::to_value(output_dir_for_ctx).unwrap(),
        );
        c.insert(
            "plan_dir".to_string(),
            serde_json::to_value(plan_dir.clone()).unwrap(),
        );
    });
    run_goal_plain(args, backend, "plan", ctx, true, &shutdown)
}

fn on_progress(_event: &ProgressEvent) {
    // Plain mode: progress is not displayed (no stdout/stderr per AGENTS.md)
}

/// Run as headless gRPC daemon. Serves GetSession and ListSessions; blocks until shutdown.
/// When LiveKit args are present, also joins the room as a participant serving RPC over the data channel.
fn run_daemon(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    tddy_core::init_tddy_logger(args.debug, args.debug_output.as_deref());

    let sessions_base = args.output_dir.clone().unwrap_or_else(|| {
        #[cfg(unix)]
        {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".tddy").join("sessions")
        }
        #[cfg(not(unix))]
        {
            PathBuf::from(".").join("sessions")
        }
    });
    std::fs::create_dir_all(&sessions_base).context("create sessions base dir")?;

    let port = args.grpc.unwrap_or(50051);
    let backend = create_backend(&args.agent, None, None);
    let service = tddy_grpc::DaemonService::new(sessions_base, backend);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("create tokio runtime")?;

    let livekit_enabled = args.livekit_url.is_some()
        && args.livekit_token.is_some()
        && args.livekit_room.is_some()
        && args.livekit_identity.is_some();

    if (args.livekit_url.is_some()
        || args.livekit_token.is_some()
        || args.livekit_room.is_some()
        || args.livekit_identity.is_some())
        && !livekit_enabled
    {
        anyhow::bail!(
            "LiveKit requires all of: --livekit-url, --livekit-token, --livekit-room, --livekit-identity"
        );
    }

    rt.block_on(async {
        let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .context("bind gRPC port")?;
        log::info!("tddy-coder daemon listening on port {}", port);

        let server = tonic::transport::Server::builder()
            .add_service(tddy_grpc::gen::tddy_remote_server::TddyRemoteServer::new(
                service,
            ))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener));

        if livekit_enabled {
            let url = args.livekit_url.as_ref().unwrap().clone();
            let token = args.livekit_token.as_ref().unwrap().clone();
            let shutdown_clone = shutdown.clone();
            tokio::spawn(async move {
                let participant = match tddy_livekit::LiveKitParticipant::connect(
                    &url,
                    &token,
                    tddy_livekit::EchoServiceImpl,
                )
                .await
                {
                    Ok(p) => {
                        log::info!("LiveKit participant connected to room");
                        p
                    }
                    Err(e) => {
                        log::error!("LiveKit connect failed: {}", e);
                        return;
                    }
                };
                tokio::select! {
                    _ = participant.run() => {
                        log::info!("LiveKit participant disconnected");
                    }
                    _ = async {
                        while !shutdown_clone.load(Ordering::Relaxed) {
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                    } => {}
                }
            });
        }

        let shutdown_fut = async {
            while !shutdown.load(Ordering::Relaxed) {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        };

        tokio::select! {
            res = server => res.context("gRPC server error")?,
            _ = shutdown_fut => {}
        }

        Ok(())
    })
}

/// Print session id and plan dir to stderr on program exit.
fn print_session_info_on_exit(plan_dir: &Path) {
    let session_id = plan_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| plan_dir.display().to_string());
    eprintln!("Session: {}", session_id);
    eprintln!("Plan dir: {}", plan_dir.display());
    let _ = std::io::stderr().flush();
}

/// Print session id and session dir path (when no plan_dir; uses startup session_id).
fn print_session_id_on_exit(session_id: &str, session_dir: &Path) {
    eprintln!("Session: {}", session_id);
    eprintln!("Plan dir: {}", session_dir.display());
    let _ = std::io::stderr().flush();
}

/// Compute session dir path from args (base/sessions/{session_id}/).
fn session_dir_path(args: &Args) -> Option<PathBuf> {
    let sid = args.session_id.as_deref()?;
    let base = if args.output_dir.is_none() {
        #[cfg(unix)]
        {
            let home = std::env::var("HOME").ok()?;
            PathBuf::from(home).join(".tddy")
        }
        #[cfg(not(unix))]
        {
            return None;
        }
    } else {
        args.output_dir.clone()?
    };
    Some(base.join(tddy_core::output::SESSIONS_SUBDIR).join(sid))
}

/// Create backend once at startup (plain mode, no progress events).
/// StubBackend always uses InMemoryToolExecutor (no tddy-tools): stub simulates the agent,
/// so it stores results directly. ProcessToolExecutor is for real agents (Claude/Cursor)
/// that run tddy-tools submit.
fn create_backend(
    agent: &str,
    _socket_path: Option<&Path>,
    _working_dir: Option<&Path>,
) -> SharedBackend {
    log::debug!("[tddy-coder] using agent: {}", agent);
    let backend: AnyBackend = match agent {
        "cursor" => AnyBackend::Cursor(CursorBackend::new().with_progress(on_progress)),
        "stub" => AnyBackend::Stub(StubBackend::new()),
        _ => AnyBackend::Claude(ClaudeCodeBackend::new().with_progress(on_progress)),
    };
    SharedBackend::from_any(backend)
}

/// Resolve conversation_output and debug_output defaults to plan_dir/logs/ when not set.
/// Returns the resolved conversation output path for use in context.
fn resolve_log_defaults(args: &Args, plan_dir: &Path) -> Option<PathBuf> {
    tddy_core::resolve_log_defaults(
        args.conversation_output.clone(),
        args.debug_output.as_ref(),
        plan_dir,
    )
}

/// Build context_values for a goal from args and plan_dir.
fn build_goal_context(
    args: &Args,
    plan_dir: Option<&PathBuf>,
    conversation_output: &Option<PathBuf>,
    extra: impl FnOnce(&mut std::collections::HashMap<String, serde_json::Value>),
) -> std::collections::HashMap<String, serde_json::Value> {
    let inherit_stdin = io::stdin().is_terminal();
    let mut ctx = std::collections::HashMap::new();
    ctx.insert(
        "model".to_string(),
        serde_json::to_value(args.model.clone()).unwrap(),
    );
    ctx.insert("agent_output".to_string(), serde_json::json!(true));
    ctx.insert(
        "conversation_output_path".to_string(),
        serde_json::to_value(conversation_output.clone()).unwrap(),
    );
    ctx.insert(
        "inherit_stdin".to_string(),
        serde_json::json!(inherit_stdin),
    );
    ctx.insert(
        "allowed_tools".to_string(),
        serde_json::to_value(args.allowed_tools.clone()).unwrap(),
    );
    ctx.insert("debug".to_string(), serde_json::json!(args.debug));
    if let Some(p) = plan_dir {
        ctx.insert("plan_dir".to_string(), serde_json::to_value(p).unwrap());
        let output_dir = args
            .output_dir
            .clone()
            .or_else(|| p.parent().map(|x| x.to_path_buf()));
        ctx.insert(
            "output_dir".to_string(),
            serde_json::to_value(output_dir).unwrap(),
        );
    }
    extra(&mut ctx);
    ctx
}

/// Run a single goal via WorkflowEngine with clarification loop. Prints output on success unless print_output is false.
/// When shutdown is set during plan approval (e.g. by SIGINT handler), prints "Session: (workflow did not complete)" and returns Ok.
fn run_goal_plain(
    args: &Args,
    backend: SharedBackend,
    goal: &str,
    context_values: std::collections::HashMap<String, serde_json::Value>,
    print_output: bool,
    shutdown: &AtomicBool,
) -> anyhow::Result<()> {
    let storage_dir = std::env::temp_dir().join("tddy-flowrunner-session");
    std::fs::create_dir_all(&storage_dir).context("create session storage dir")?;
    let hooks = std::sync::Arc::new(tddy_core::workflow::tdd_hooks::TddWorkflowHooks::new());
    let engine = WorkflowEngine::new(backend.clone(), storage_dir, Some(hooks));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("create tokio runtime")?;

    let mut result = rt
        .block_on(engine.run_goal(goal, context_values.clone()))
        .map_err(|e| anyhow::anyhow!("WorkflowEngine: {}", e))?;

    loop {
        match &result.status {
            ExecutionStatus::Completed | ExecutionStatus::Paused { .. } => {
                let session_opt = rt
                    .block_on(engine.get_session(&result.session_id))
                    .map_err(|e| anyhow::anyhow!("get session: {}", e))?;
                let plan_dir: PathBuf = session_opt
                    .as_ref()
                    .and_then(|s| {
                        s.context
                            .get_sync("plan_dir")
                            .or_else(|| s.context.get_sync("output_dir"))
                    })
                    .unwrap_or_else(|| {
                        args.plan_dir.clone().unwrap_or_else(|| {
                            args.output_dir
                                .clone()
                                .unwrap_or_else(|| PathBuf::from("."))
                        })
                    });
                let output: Option<String> = session_opt
                    .as_ref()
                    .and_then(|s| s.context.get_sync("output"));

                if print_output {
                    print_goal_output(goal, output.as_deref(), &plan_dir)?;
                }
                print_session_info_on_exit(&plan_dir);
                return Ok(());
            }
            ExecutionStatus::ElicitationNeeded { ref event } => {
                let plan_dir: PathBuf = rt
                    .block_on(engine.get_session(&result.session_id))
                    .ok()
                    .flatten()
                    .and_then(|s| {
                        s.context
                            .get_sync("plan_dir")
                            .or_else(|| s.context.get_sync("output_dir"))
                    })
                    .unwrap_or_else(|| {
                        args.plan_dir.clone().unwrap_or_else(|| {
                            args.output_dir
                                .clone()
                                .unwrap_or_else(|| PathBuf::from("."))
                        })
                    });
                match event {
                    tddy_core::ElicitationEvent::PlanApproval { ref prd_content } => {
                        let mut current_prd = prd_content.clone();
                        loop {
                            let answer = match plain::read_plan_approval_plain(&current_prd) {
                                Ok(a) => a,
                                Err(e) => {
                                    if e.downcast_ref::<std::io::Error>()
                                        .is_some_and(|io| io.kind() == io::ErrorKind::Interrupted)
                                        && shutdown.load(Ordering::Relaxed)
                                    {
                                        if let Some(sid) = args.session_id.as_ref() {
                                            let dir = session_dir_path(args).unwrap_or_else(|| {
                                                PathBuf::from("(session dir not created)")
                                            });
                                            print_session_id_on_exit(sid, &dir);
                                        }
                                        return Ok(());
                                    }
                                    return Err(e.context("plan approval"));
                                }
                            };
                            if answer.eq_ignore_ascii_case("approve") {
                                break;
                            }
                            run_plan_refinement(args, &backend, &rt, &plan_dir, &answer)?;
                            current_prd = std::fs::read_to_string(plan_dir.join("PRD.md"))
                                .unwrap_or_else(|_| "Could not read PRD.md".to_string());
                        }
                    }
                    tddy_core::ElicitationEvent::WorktreeConfirmation { .. } => {
                        anyhow::bail!(
                            "WorktreeConfirmation not supported in plain mode; use --daemon"
                        );
                    }
                }
                if print_output {
                    let session_opt = rt
                        .block_on(engine.get_session(&result.session_id))
                        .ok()
                        .flatten();
                    let output: Option<String> = session_opt
                        .as_ref()
                        .and_then(|s| s.context.get_sync("output"));
                    print_goal_output(goal, output.as_deref(), &plan_dir)?;
                }
                print_session_info_on_exit(&plan_dir);
                return Ok(());
            }
            ExecutionStatus::WaitingForInput { .. } => {
                let session = rt
                    .block_on(engine.get_session(&result.session_id))
                    .map_err(|e| anyhow::anyhow!("get session: {}", e))?
                    .ok_or_else(|| anyhow::anyhow!("session not found"))?;
                let questions: Vec<tddy_core::ClarificationQuestion> = session
                    .context
                    .get_sync("pending_questions")
                    .ok_or_else(|| anyhow::anyhow!("no pending questions"))?;
                let answers = plain::read_answers_plain(&questions).context("read answers")?;
                let mut updates = std::collections::HashMap::new();
                updates.insert("answers".to_string(), serde_json::json!(answers));
                rt.block_on(engine.update_session_context(&result.session_id, updates))
                    .map_err(|e| anyhow::anyhow!("update session: {}", e))?;
                result = rt
                    .block_on(engine.run_session(&result.session_id))
                    .map_err(|e| anyhow::anyhow!("run session: {}", e))?;
            }
            ExecutionStatus::Error(msg) => anyhow::bail!("Workflow error: {}", msg),
        }
    }
}

fn print_goal_output(goal: &str, output: Option<&str>, plan_dir: &Path) -> anyhow::Result<()> {
    match goal {
        "plan" => {
            // Plan goal: print only the path (CLI contract for piping/scripts)
            println!("{}", plan_dir.display());
            return Ok(());
        }
        "acceptance-tests" => {
            let out = output
                .and_then(|s| parse_acceptance_tests_response(s).ok())
                .ok_or_else(|| anyhow::anyhow!("no parseable acceptance-tests output"))?;
            println!("{}", out.summary);
            for t in &out.tests {
                println!(
                    "  - {} ({}:{}): {}",
                    t.name,
                    t.file,
                    t.line.unwrap_or(0),
                    t.status
                );
            }
            if let Some(ref cmd) = out.test_command {
                println!("\nHow to run tests: {}", cmd);
            }
            if let Some(ref prereq) = out.prerequisite_actions {
                println!("Prerequisite actions: {}", prereq);
            }
            if let Some(ref single) = out.run_single_or_selected_tests {
                println!("How to run a single or selected tests: {}", single);
            }
        }
        "red" => {
            let out = output
                .and_then(|s| parse_red_response(s).ok())
                .ok_or_else(|| anyhow::anyhow!("no parseable red output"))?;
            println!("{}", out.summary);
            for t in &out.tests {
                println!(
                    "  - {} ({}:{}): {}",
                    t.name,
                    t.file,
                    t.line.unwrap_or(0),
                    t.status
                );
            }
            for s in &out.skeletons {
                println!(
                    "  [skeleton] {} ({}:{}): {}",
                    s.name,
                    s.file,
                    s.line.unwrap_or(0),
                    s.kind
                );
            }
            if let Some(ref cmd) = out.test_command {
                println!("\nHow to run tests: {}", cmd);
            }
            if let Some(ref prereq) = out.prerequisite_actions {
                println!("Prerequisite actions: {}", prereq);
            }
            if let Some(ref single) = out.run_single_or_selected_tests {
                println!("How to run a single or selected tests: {}", single);
            }
        }
        "green" => {
            let out = output
                .and_then(|s| parse_green_response(s).ok())
                .ok_or_else(|| anyhow::anyhow!("no parseable green output"))?;
            println!("{}", out.summary);
            for t in &out.tests {
                println!(
                    "  - {} ({}:{}): {}",
                    t.name,
                    t.file,
                    t.line.unwrap_or(0),
                    t.status
                );
            }
            for i in &out.implementations {
                println!(
                    "  [impl] {} ({}:{}): {}",
                    i.name,
                    i.file,
                    i.line.unwrap_or(0),
                    i.kind
                );
            }
            if let Some(ref cmd) = out.test_command {
                println!("\nHow to run tests: {}", cmd);
            }
            if let Some(ref prereq) = out.prerequisite_actions {
                println!("Prerequisite actions: {}", prereq);
            }
            if let Some(ref single) = out.run_single_or_selected_tests {
                println!("How to run a single or selected tests: {}", single);
            }
        }
        "evaluate" => {
            let out = output
                .and_then(|s| parse_evaluate_response(s).ok())
                .ok_or_else(|| anyhow::anyhow!("no parseable evaluate output"))?;
            println!("{}", out.summary);
            println!("Risk level: {}", out.risk_level);
            println!(
                "Report: {}",
                plan_dir.join("evaluation-report.md").display()
            );
        }
        "demo" => {
            let out = output
                .and_then(|s| tddy_core::parse_demo_response(s).ok())
                .ok_or_else(|| anyhow::anyhow!("no parseable demo output"))?;
            println!("{}", out.summary);
            println!("Steps completed: {}", out.steps_completed);
        }
        "validate" => {
            let out = output
                .and_then(|s| parse_validate_subagents_response(s).ok())
                .ok_or_else(|| anyhow::anyhow!("no parseable validate output"))?;
            println!("{}", out.summary);
        }
        "refactor" => {
            let out = output
                .and_then(|s| parse_refactor_response(s).ok())
                .ok_or_else(|| anyhow::anyhow!("no parseable refactor output"))?;
            println!("{}", out.summary);
            println!("Tasks completed: {}", out.tasks_completed);
            println!("Tests passing: {}", out.tests_passing);
        }
        "update-docs" => {
            let out = output
                .and_then(|s| parse_update_docs_response(s).ok())
                .ok_or_else(|| anyhow::anyhow!("no parseable update-docs output"))?;
            println!("{}", out.summary);
            println!("Docs updated: {}", out.docs_updated);
        }
        _ => {}
    }
    println!("\nPlan dir: {}", plan_dir.display());
    Ok(())
}

fn run_full_workflow_tui(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    std::env::set_var("TDDY_QUIET", "1");
    log::set_max_level(log::LevelFilter::Debug);

    let (socket_path, tool_call_rx) = match tddy_core::toolcall::start_toolcall_listener() {
        Ok((path, rx)) => (Some(path), Some(rx)),
        Err(_) => (None, None),
    };
    let backend = create_backend(
        &args.agent,
        socket_path.as_deref(),
        args.output_dir.as_deref(),
    );
    let view = tddy_tui::TuiView::new();
    let mut presenter = Presenter::new(view, &args.agent, args.model.as_deref().unwrap_or("opus"));

    let (external_intent_rx, byte_capture) = if let Some(port) = args.grpc {
        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        let (intent_tx, intent_rx) = std::sync::mpsc::channel();
        let (terminal_byte_tx, _) = tokio::sync::broadcast::channel(256);
        let handle = tddy_core::PresenterHandle {
            event_tx: event_tx.clone(),
            intent_tx: intent_tx.clone(),
        };
        presenter = presenter.with_broadcast(event_tx);
        let service =
            tddy_grpc::TddyRemoteService::new(handle).with_terminal_bytes(terminal_byte_tx.clone());
        let byte_tx_for_callback = terminal_byte_tx.clone();
        let byte_capture: Option<tddy_tui::ByteCallback> = Some(Box::new(move |buf: &[u8]| {
            let _ = byte_tx_for_callback.send(buf.to_vec());
        }));
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let result: anyhow::Result<()> = rt.block_on(async move {
                let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
                let listener = tokio::net::TcpListener::bind(addr).await?;
                log::info!("gRPC server listening on port {}", port);
                tonic::transport::Server::builder()
                    .add_service(tddy_grpc::gen::tddy_remote_server::TddyRemoteServer::new(
                        service,
                    ))
                    .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                    .await
                    .map_err(anyhow::Error::from)
            });
            result.expect("gRPC server failed")
        });
        (Some(intent_rx), byte_capture)
    } else {
        (None, None)
    };

    if let Some(ref output_dir) = args.output_dir {
        reject_output_dir_if_repo_root(output_dir)?;
    }
    let initial_prompt = args.prompt.clone();
    presenter.start_workflow(
        backend,
        args.output_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(".")),
        initial_prompt,
        args.conversation_output.clone(),
        args.debug_output.clone(),
        args.debug,
        args.session_id.clone(),
        socket_path,
        tool_call_rx,
    );

    tddy_tui::run_event_loop(
        &mut presenter,
        shutdown.as_ref(),
        external_intent_rx,
        byte_capture,
        args.debug,
    )?;

    if let Some(result) = presenter.take_workflow_result() {
        match &result {
            Ok(payload) => {
                println!("{}", payload.summary);
                let _ = std::io::stdout().flush();
                if let Some(ref plan_dir) = payload.plan_dir {
                    print_session_info_on_exit(plan_dir);
                }
            }
            Err(e) => {
                eprintln!("Workflow error: {}", e);
                let _ = std::io::stderr().flush();
            }
        }
    } else {
        if let Some(sid) = args.session_id.as_ref() {
            let dir = session_dir_path(args)
                .unwrap_or_else(|| PathBuf::from("(session dir not created)"));
            print_session_id_on_exit(sid, &dir);
        }
    }

    Ok(())
}

fn run_full_workflow_plain(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    let backend = create_backend(&args.agent, None, None);

    let mut plan_dir = if let Some(ref p) = args.plan_dir {
        p.clone()
    } else {
        run_plan_to_get_dir(args, backend.clone(), &shutdown)?
    };

    // When resuming with --plan-dir: if state is Init and plan is incomplete, run plan to complete it.
    let cs_pre = read_changeset(&plan_dir).ok();
    let plan_needs_completion = cs_pre.as_ref().is_some_and(|c| {
        c.state.current == "Init"
            && (!plan_dir.join("PRD.md").exists() || get_session_for_tag(c, "plan").is_none())
    });
    if plan_needs_completion {
        let input = cs_pre
            .as_ref()
            .and_then(|c| c.initial_prompt.as_deref())
            .unwrap_or("feature")
            .trim()
            .to_string();
        if !input.is_empty() {
            plan_dir = run_plan_to_complete(args, backend.clone(), &input, &plan_dir, &shutdown)?;
        }
    }

    let run_demo = plan_dir.join("demo-plan.md").exists()
        && plain::read_demo_choice_plain().context("read demo choice")?;

    let cs = read_changeset(&plan_dir).ok();
    let start_goal = cs
        .as_ref()
        .and_then(|c| next_goal_for_state(&c.state.current))
        .unwrap_or("plan");

    let storage_dir = std::env::temp_dir().join("tddy-flowrunner-session");
    std::fs::create_dir_all(&storage_dir).context("create session storage dir")?;
    let hooks = std::sync::Arc::new(tddy_core::workflow::tdd_hooks::TddWorkflowHooks::new());
    let backend_for_refine = backend.clone();
    let engine = WorkflowEngine::new(backend, storage_dir, Some(hooks));

    let feature_input = cs_pre
        .as_ref()
        .and_then(|c| c.initial_prompt.as_deref())
        .or(args.prompt.as_deref())
        .unwrap_or("feature")
        .trim()
        .to_string();
    let conv = resolve_log_defaults(args, &plan_dir);
    let output_dir = args
        .output_dir
        .clone()
        .or_else(|| plan_dir.parent().map(|p| p.to_path_buf()));
    let context_values = build_goal_context(args, Some(&plan_dir), &conv, |c| {
        c.insert(
            "feature_input".to_string(),
            serde_json::json!(feature_input),
        );
        c.insert("run_demo".to_string(), serde_json::json!(run_demo));
        c.insert(
            "output_dir".to_string(),
            serde_json::to_value(output_dir).unwrap(),
        );
    });

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("create tokio runtime")?;

    let mut result = if start_goal == "plan" {
        rt.block_on(engine.run_full_workflow(context_values))
    } else {
        rt.block_on(engine.run_workflow_from(start_goal, context_values))
    }
    .map_err(|e| anyhow::anyhow!("WorkflowEngine: {}", e))?;

    loop {
        match &result.status {
            ExecutionStatus::Completed | ExecutionStatus::Paused { .. } => {
                let session_opt = rt
                    .block_on(engine.get_session(&result.session_id))
                    .map_err(|e| anyhow::anyhow!("get session: {}", e))?;
                let output: Option<String> = session_opt
                    .as_ref()
                    .and_then(|s| s.context.get_sync("output"));
                let plan_dir_final: PathBuf = session_opt
                    .as_ref()
                    .and_then(|s| {
                        s.context
                            .get_sync("plan_dir")
                            .or_else(|| s.context.get_sync("output_dir"))
                    })
                    .unwrap_or(plan_dir.clone());
                if let Some(ref out) = output {
                    if let Ok(refactor_out) = parse_refactor_response(out) {
                        if let Ok(eval_content) =
                            std::fs::read_to_string(plan_dir_final.join("evaluation-report.md"))
                        {
                            if let Ok(eval_out) = parse_evaluate_response(&eval_content) {
                                println!("Evaluation: {}", eval_out.summary);
                            }
                        }
                        println!("{}", refactor_out.summary);
                        println!("Tasks completed: {}", refactor_out.tasks_completed);
                        println!("Tests passing: {}", refactor_out.tests_passing);
                    }
                }
                println!("\nPlan dir: {}", plan_dir_final.display());
                print_session_info_on_exit(&plan_dir_final);
                return Ok(());
            }
            ExecutionStatus::WaitingForInput { .. } => {
                let session = rt
                    .block_on(engine.get_session(&result.session_id))
                    .map_err(|e| anyhow::anyhow!("get session: {}", e))?
                    .ok_or_else(|| anyhow::anyhow!("session not found"))?;
                let questions: Vec<tddy_core::ClarificationQuestion> = session
                    .context
                    .get_sync("pending_questions")
                    .ok_or_else(|| anyhow::anyhow!("no pending questions"))?;
                let answers = plain::read_answers_plain(&questions).context("read answers")?;
                let mut updates = std::collections::HashMap::new();
                updates.insert("answers".to_string(), serde_json::json!(answers));
                rt.block_on(engine.update_session_context(&result.session_id, updates))
                    .map_err(|e| anyhow::anyhow!("update session: {}", e))?;
                result = rt
                    .block_on(engine.run_session(&result.session_id))
                    .map_err(|e| anyhow::anyhow!("run session: {}", e))?;
            }
            ExecutionStatus::Error(msg) => anyhow::bail!("Workflow error: {}", msg),
            ExecutionStatus::ElicitationNeeded { ref event } => {
                match event {
                    tddy_core::ElicitationEvent::PlanApproval { ref prd_content } => {
                        let mut current_prd = prd_content.clone();
                        loop {
                            let answer = plain::read_plan_approval_plain(&current_prd)
                                .context("plan approval")?;
                            if answer.eq_ignore_ascii_case("approve") {
                                break;
                            }
                            run_plan_refinement(
                                args,
                                &backend_for_refine,
                                &rt,
                                &plan_dir,
                                &answer,
                            )?;
                            current_prd = std::fs::read_to_string(plan_dir.join("PRD.md"))
                                .unwrap_or_else(|_| "Could not read PRD.md".to_string());
                        }
                    }
                    tddy_core::ElicitationEvent::WorktreeConfirmation { .. } => {
                        anyhow::bail!(
                            "WorktreeConfirmation not supported in plain mode; use --daemon"
                        );
                    }
                }
                result = rt
                    .block_on(engine.run_session(&result.session_id))
                    .map_err(|e| anyhow::anyhow!("run session: {}", e))?;
            }
        }
    }
}

fn run_plan_to_get_dir(
    args: &Args,
    backend: SharedBackend,
    shutdown: &AtomicBool,
) -> anyhow::Result<PathBuf> {
    let input = read_feature_input(args).context("read feature description")?;
    let input = input.trim().to_string();
    if input.is_empty() {
        anyhow::bail!("empty feature description");
    }
    let (plan_dir, output_dir_for_ctx) = if let Some(output_dir) = &args.output_dir {
        reject_output_dir_if_repo_root(output_dir)?;
        let plan_dir = output_dir.join(tddy_core::output::slugify_directory_name(&input));
        std::fs::create_dir_all(&plan_dir).context("create plan directory")?;
        let output_dir_for_ctx = output_dir.clone();
        (plan_dir, output_dir_for_ctx)
    } else {
        #[cfg(unix)]
        {
            let home = std::env::var("HOME").map_err(|_| {
                anyhow::anyhow!("HOME not set; cannot create session under ~/.tddy")
            })?;
            let base = PathBuf::from(&home).join(".tddy");
            let plan_dir =
                tddy_core::output::create_session_dir_in(&base).context("create session dir")?;
            let output_dir_for_ctx =
                std::env::current_dir().context("current dir for agent working_dir")?;
            (plan_dir, output_dir_for_ctx)
        }
        #[cfg(not(unix))]
        {
            anyhow::bail!(
                "plan without --output-dir requires HOME (Unix) or USERPROFILE (Windows); \
                 use --output-dir <path> explicitly"
            );
        }
    };
    let conv = resolve_log_defaults(args, &plan_dir);
    let ctx = build_goal_context(args, None, &conv, |c| {
        c.insert("feature_input".to_string(), serde_json::json!(input));
        c.insert(
            "output_dir".to_string(),
            serde_json::to_value(output_dir_for_ctx).unwrap(),
        );
        c.insert(
            "plan_dir".to_string(),
            serde_json::to_value(plan_dir.clone()).unwrap(),
        );
    });
    run_goal_plain(args, backend, "plan", ctx, false, shutdown)?;
    Ok(plan_dir)
}

fn run_plan_to_complete(
    args: &Args,
    backend: SharedBackend,
    input: &str,
    plan_dir: &PathBuf,
    shutdown: &AtomicBool,
) -> anyhow::Result<PathBuf> {
    let output_dir = plan_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| plan_dir.clone());
    let conv = resolve_log_defaults(args, plan_dir);
    let ctx = build_goal_context(args, Some(plan_dir), &conv, |c| {
        c.insert("feature_input".to_string(), serde_json::json!(input));
        c.insert(
            "output_dir".to_string(),
            serde_json::to_value(output_dir).unwrap(),
        );
    });
    run_goal_plain(args, backend, "plan", ctx, false, shutdown)?;
    Ok(plan_dir.clone())
}

/// Run plan refinement: re-run the plan goal with feedback, handling clarification.
fn run_plan_refinement(
    args: &Args,
    backend: &SharedBackend,
    rt: &tokio::runtime::Runtime,
    plan_dir: &Path,
    feedback: &str,
) -> anyhow::Result<()> {
    let feature_input = read_changeset(plan_dir)
        .ok()
        .and_then(|c| c.initial_prompt.clone())
        .unwrap_or_else(|| "feature".to_string());
    let session_id_for_refine = read_changeset(plan_dir)
        .ok()
        .and_then(|c| get_session_for_tag(&c, "plan"));
    let output_dir = plan_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| plan_dir.to_path_buf());
    let refine_storage = std::env::temp_dir().join("tddy-flowrunner-refine-session");
    std::fs::create_dir_all(&refine_storage).context("create refine session dir")?;
    let refine_hooks = std::sync::Arc::new(tddy_core::workflow::tdd_hooks::TddWorkflowHooks::new());
    let refine_engine = WorkflowEngine::new(backend.clone(), refine_storage, Some(refine_hooks));
    let plan_dir_buf = plan_dir.to_path_buf();
    let conv = resolve_log_defaults(args, &plan_dir_buf);
    let mut refine_ctx = build_goal_context(args, Some(&plan_dir_buf), &conv, |c| {
        c.insert(
            "feature_input".to_string(),
            serde_json::json!(feature_input),
        );
        c.insert(
            "output_dir".to_string(),
            serde_json::to_value(&output_dir).unwrap(),
        );
        c.insert(
            "refinement_feedback".to_string(),
            serde_json::json!(feedback),
        );
    });
    if let Some(sid) = session_id_for_refine {
        refine_ctx.insert("session_id".to_string(), serde_json::json!(sid));
    }
    let mut refine_result = rt
        .block_on(refine_engine.run_goal("plan", refine_ctx))
        .map_err(|e| anyhow::anyhow!("refinement: {}", e))?;
    loop {
        match &refine_result.status {
            ExecutionStatus::Completed
            | ExecutionStatus::Paused { .. }
            | ExecutionStatus::ElicitationNeeded { .. } => break,
            ExecutionStatus::WaitingForInput { .. } => {
                let session = rt
                    .block_on(refine_engine.get_session(&refine_result.session_id))
                    .map_err(|e| anyhow::anyhow!("get session: {}", e))?
                    .ok_or_else(|| anyhow::anyhow!("session not found"))?;
                let questions: Vec<tddy_core::ClarificationQuestion> = session
                    .context
                    .get_sync("pending_questions")
                    .ok_or_else(|| anyhow::anyhow!("no pending questions"))?;
                let answers = plain::read_answers_plain(&questions).context("read answers")?;
                let mut updates = std::collections::HashMap::new();
                updates.insert("answers".to_string(), serde_json::json!(answers));
                rt.block_on(
                    refine_engine.update_session_context(&refine_result.session_id, updates),
                )
                .map_err(|e| anyhow::anyhow!("update session: {}", e))?;
                refine_result = rt
                    .block_on(refine_engine.run_session(&refine_result.session_id))
                    .map_err(|e| anyhow::anyhow!("run session: {}", e))?;
            }
            ExecutionStatus::Error(msg) => anyhow::bail!("Refinement error: {}", msg),
        }
    }
    Ok(())
}

/// Read feature description. Uses --prompt if set; otherwise stdin.
fn read_feature_input(args: &Args) -> anyhow::Result<String> {
    if let Some(ref p) = args.prompt {
        return Ok(p.clone());
    }
    let mut buf = String::new();
    io::stdin().lock().read_to_string(&mut buf)?;
    Ok(buf)
}
