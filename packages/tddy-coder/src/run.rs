//! Run logic shared by tddy-coder and tddy-demo binaries.
//!
//! Args is the common runtime type. CoderArgs and DemoArgs are CLI parser types
//! with different agent constraints; both convert to Args via From.

use anyhow::Context;
use clap::Parser;
use std::io::{self, IsTerminal, Read, Write};
#[cfg(unix)]
use std::os::fd::IntoRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::{
    backend_from_label, backend_selection_question, default_model_for_agent, get_session_for_tag,
    next_goal_for_state, parse_acceptance_tests_response, parse_evaluate_response,
    parse_green_response, parse_red_response, parse_refactor_response, parse_update_docs_response,
    parse_validate_subagents_response, preselected_index_for_agent, read_changeset, AnyBackend,
    ClaudeAcpBackend, ClaudeCodeBackend, CodingBackend, CursorBackend, PendingWorkflowStart,
    ProgressEvent, SharedBackend, StubBackend, WorkflowEngine,
};

use crate::plain;
use crate::tty::should_run_tui;
use tddy_core::Presenter;

use crate::disable_raw_mode;

/// TokenProvider that delegates to TokenGenerator. Used when the daemon has API key/secret.
struct LiveKitTokenProvider(std::sync::Arc<tddy_livekit::TokenGenerator>);

impl tddy_service::TokenProvider for LiveKitTokenProvider {
    fn generate_token(&self, room: &str, identity: &str) -> Result<String, String> {
        self.0
            .generate_for(room, identity)
            .map_err(|e| e.to_string())
    }
    fn ttl_seconds(&self) -> u64 {
        self.0.ttl().as_secs()
    }
}

/// Verify tddy-tools binary is available. Required for claude/cursor agents.
/// Skips when agent is stub (uses InMemoryToolExecutor).
fn verify_tddy_tools_available(agent: &str) -> anyhow::Result<()> {
    if agent == "stub" || agent == "claude-acp" {
        return Ok(());
    }
    // Check 1: Same directory as current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("tddy-tools").exists() {
                return Ok(());
            }
        }
    }
    // Check 2: On PATH
    match std::process::Command::new("tddy-tools")
        .arg("--help")
        .output()
    {
        Ok(output) if output.status.success() => Ok(()),
        _ => anyhow::bail!(
            "tddy-tools binary not found. Build it with: cargo build -p tddy-tools\n\
             Or ensure it's on PATH."
        ),
    }
}

fn assign_default_session_id(args: &mut Args) {
    if args.session_id.is_some() {
        return;
    }
    if let Some(ref sid) = args.resume_from {
        args.session_id = Some(sid.clone());
        return;
    }
    args.session_id = Some(uuid::Uuid::now_v7().to_string());
}

/// Plain stdin menu when `--agent` was omitted (single-goal mode).
fn resolve_agent_for_single_goal_plain(_args: &Args) -> anyhow::Result<String> {
    let q = backend_selection_question();
    let label = plain::read_backend_selection_plain(&q)?;
    let (agent, _) = backend_from_label(&label);
    verify_tddy_tools_available(agent)?;
    Ok(agent.to_string())
}

/// Plain stdin menu when `--agent` was omitted (full workflow, no TUI).
fn resolve_agent_for_full_workflow_plain(args: &Args) -> anyhow::Result<String> {
    if let Some(ref a) = args.agent {
        return Ok(a.clone());
    }
    let q = backend_selection_question();
    let label = plain::read_backend_selection_plain(&q)?;
    let (agent, _) = backend_from_label(&label);
    verify_tddy_tools_available(agent)?;
    Ok(agent.to_string())
}

/// Shared main entry: panic hook, Ctrl+C handler, run_with_args, exit logic.
/// Use from both tddy-coder and tddy-demo binaries.
pub fn run_main(mut args: Args) {
    assign_default_session_id(&mut args);

    if let Err(e) = merge_session_coder_config_for_resume(&mut args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = apply_plan_dir_from_session_if_needed(&mut args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = apply_agent_from_changeset_if_needed(&mut args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    // Validate args before any stderr redirect (daemon redirects stderr to a file).
    if let Err(e) = validate_web_args(&args).and_then(|_| validate_livekit_args(&args)) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    let log_config = effective_log_config(&args);
    let has_file_output = tddy_core::config_has_file_output(&log_config);
    tddy_core::init_tddy_logger(log_config);
    if let Some(session_dir) = session_dir_path(&args) {
        let logs = session_dir.join("logs");
        let _ = std::fs::create_dir_all(&logs);
        if !has_file_output {
            tddy_core::redirect_debug_output(&logs.join("debug.log"));
        }
        log::set_max_level(log::LevelFilter::Debug);
        // Daemon runs headless (stdin/stdout/stderr = null). Redirect stderr to a real file
        // so crossterm/terminal APIs that may probe stderr work correctly (e.g. VirtualTui).
        #[cfg(unix)]
        if args.daemon {
            if let Ok(file) = std::fs::File::create(logs.join("daemon_stderr.log")) {
                let fd = file.into_raw_fd();
                let _ = unsafe { libc::dup2(fd, libc::STDERR_FILENO) };
                let _ = unsafe { libc::close(fd) };
            }
        }
    }

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

    let result = run_with_args(&args, shutdown);

    match result {
        Err(e) => {
            // Print session info on error (e.g. SIGINT) so user knows where to find the session.
            if let Some(sid) = args.session_id.as_ref() {
                if let Some(dir) = session_dir_path(&args) {
                    print_session_id_on_exit(sid, &dir);
                }
            }
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
    pub plan_dir: Option<PathBuf>,
    pub conversation_output: Option<PathBuf>,
    pub model: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    /// Log config from YAML. When None, default_log_config is used.
    pub log: Option<tddy_core::LogConfig>,
    /// CLI override for default log level (e.g. --log-level debug).
    pub log_level: Option<log::LevelFilter>,
    /// When `None`, the user did not pass `--agent` (interactive backend selection).
    pub agent: Option<String>,
    pub prompt: Option<String>,
    /// When Some(port), gRPC server runs alongside TUI on the given port.
    pub grpc: Option<u16>,
    /// Session ID set at program start; used for exit output when no plan_dir.
    pub session_id: Option<String>,
    /// Resume from existing session (session ID). Sets plan_dir to session dir.
    pub resume_from: Option<String>,
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
    /// LiveKit API key for token generation (mutually exclusive with --livekit-token)
    pub livekit_api_key: Option<String>,
    /// LiveKit API secret for token generation (mutually exclusive with --livekit-token)
    pub livekit_api_secret: Option<String>,
    /// Public LiveKit URL for browser clients (e.g. ws://192.168.1.10:7880). Defaults to livekit_url.
    pub livekit_public_url: Option<String>,
    /// When Some with web_bundle_path, HTTP server serves static files on this port.
    pub web_port: Option<u16>,
    /// Path to pre-built web bundle (e.g. packages/tddy-web/dist). Requires web_port.
    pub web_bundle_path: Option<PathBuf>,
    /// Host to bind web server (e.g. 127.0.0.1 or 0.0.0.0). Default: 0.0.0.0 when web server is enabled.
    pub web_host: Option<String>,
    /// Public URL for browser-facing redirects (e.g. http://192.168.1.10:8899).
    pub web_public_url: Option<String>,
    /// GitHub OAuth client ID
    pub github_client_id: Option<String>,
    /// GitHub OAuth client secret
    pub github_client_secret: Option<String>,
    /// GitHub OAuth redirect URI (default: http://localhost:{web_port}/auth/callback)
    pub github_redirect_uri: Option<String>,
    /// Use stub GitHub provider instead of real GitHub
    pub github_stub: bool,
    /// Pre-register code:user mappings for stub (e.g. "test-code:testuser")
    pub github_stub_codes: Option<String>,
    /// Enable mouse/touch mode in the TUI
    pub mouse: bool,
    /// Project ID for daemon sessions (set by tddy-daemon when spawning).
    pub project_id: Option<String>,

    /// Path to the Cursor `agent` CLI. When set, overrides `TDDY_CURSOR_AGENT` and the default `agent` on `PATH`.
    pub cursor_agent_path: Option<PathBuf>,
}

/// CLI args for tddy-coder binary: agent is claude or cursor.
#[derive(Parser, Debug, Clone)]
#[command(name = "tddy-coder")]
#[command(about = "TDD-driven coder for PRD-based development workflow")]
pub struct CoderArgs {
    /// Path to YAML config file (e.g. -c config.yaml). CLI args override config values.
    #[arg(short = 'c', long = "config")]
    pub config: Option<PathBuf>,

    /// Goal to execute: plan, acceptance-tests, red, green, demo, evaluate, validate, refactor. Omit to run full workflow.
    #[arg(long, value_parser = ["plan", "acceptance-tests", "red", "green", "demo", "evaluate", "validate", "refactor", "update-docs"])]
    pub goal: Option<String>,

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

    /// Override default log level (e.g. debug, trace)
    #[arg(long, value_name = "LEVEL", value_parser = ["off", "error", "warn", "info", "debug", "trace"])]
    pub log_level: Option<String>,

    /// Agent backend: claude, claude-acp, cursor, or stub. Omit to choose interactively at startup.
    #[arg(long, value_parser = ["claude", "claude-acp", "cursor", "stub"])]
    pub agent: Option<String>,

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

    /// LiveKit API key for token generation (mutually exclusive with --livekit-token)
    #[arg(long, env = "LIVEKIT_API_KEY")]
    pub livekit_api_key: Option<String>,

    /// LiveKit API secret for token generation (mutually exclusive with --livekit-token)
    #[arg(long, env = "LIVEKIT_API_SECRET")]
    pub livekit_api_secret: Option<String>,

    /// Public LiveKit URL for browser clients (e.g. ws://192.168.1.10:7880). Defaults to --livekit-url.
    #[arg(long)]
    pub livekit_public_url: Option<String>,

    /// Port for HTTP static file server (serves --web-bundle-path). Requires --web-bundle-path.
    #[arg(long, value_name = "PORT")]
    pub web_port: Option<u16>,

    /// Path to pre-built web bundle (e.g. packages/tddy-web/dist). Requires --web-port.
    #[arg(long)]
    pub web_bundle_path: Option<PathBuf>,

    /// Host to bind web server (e.g. 127.0.0.1 or 0.0.0.0). Default: 0.0.0.0 when web server is enabled.
    #[arg(long, value_name = "HOST")]
    pub web_host: Option<String>,

    /// Public URL for browser-facing redirects (e.g. http://192.168.1.10:8899)
    #[arg(long)]
    pub web_public_url: Option<String>,

    /// GitHub OAuth client ID
    #[arg(long, env = "GITHUB_CLIENT_ID")]
    pub github_client_id: Option<String>,

    /// GitHub OAuth client secret
    #[arg(long, env = "GITHUB_CLIENT_SECRET")]
    pub github_client_secret: Option<String>,

    /// GitHub OAuth redirect URI (default: http://localhost:{web_port}/auth/callback)
    #[arg(long)]
    pub github_redirect_uri: Option<String>,

    /// Use stub GitHub OAuth provider (for testing)
    #[arg(long)]
    pub github_stub: bool,

    /// Pre-register stub code:user mappings (e.g. "test-code:testuser")
    #[arg(long)]
    pub github_stub_codes: Option<String>,

    /// Enable mouse/touch mode in the TUI
    #[arg(long)]
    pub mouse: bool,

    /// Resume from an existing session (session ID). Used when spawned by tddy-daemon.
    #[arg(long, value_name = "SESSION_ID")]
    pub resume_from: Option<String>,

    /// Session ID for new daemon sessions. Used when spawned by tddy-daemon.
    #[arg(long, value_name = "SESSION_ID")]
    pub session_id: Option<String>,

    /// Project ID for daemon sessions. Used when spawned by tddy-daemon.
    #[arg(long, value_name = "PROJECT_ID")]
    pub project_id: Option<String>,

    /// Path to the Cursor `agent` CLI (defaults to `agent` on `PATH`, or `TDDY_CURSOR_AGENT` if set).
    #[arg(long, value_name = "PATH")]
    pub cursor_agent_path: Option<PathBuf>,
}

/// CLI args for tddy-demo binary: agent is stub only.
#[derive(Parser, Debug, Clone)]
#[command(name = "tddy-demo")]
#[command(about = "Same app as tddy-coder with StubBackend (identical TUI, CLI, workflow)")]
pub struct DemoArgs {
    /// Path to YAML config file (e.g. -c config.yaml). CLI args override config values.
    #[arg(short = 'c', long = "config")]
    pub config: Option<PathBuf>,

    /// Goal to execute: plan, acceptance-tests, red, green, demo, evaluate, validate, refactor. Omit to run full workflow.
    #[arg(long, value_parser = ["plan", "acceptance-tests", "red", "green", "demo", "evaluate", "validate", "refactor", "update-docs"])]
    pub goal: Option<String>,

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

    /// Override default log level (e.g. debug, trace)
    #[arg(long, value_name = "LEVEL", value_parser = ["off", "error", "warn", "info", "debug", "trace"])]
    pub log_level: Option<String>,

    /// Agent backend: stub only. Omit defaults to stub (no interactive menu in tddy-demo).
    #[arg(long, value_parser = ["stub"])]
    pub agent: Option<String>,

    /// Feature description (alternative to stdin). When set, skips interactive/piped input.
    #[arg(long)]
    pub prompt: Option<String>,

    /// Start gRPC server alongside TUI for programmatic remote control (e.g. --grpc 50052)
    #[arg(long, value_name = "PORT", default_missing_value = "50051")]
    pub grpc: Option<u16>,

    /// Run as headless gRPC daemon (systemd-friendly)
    #[arg(long)]
    pub daemon: bool,

    /// LiveKit WebSocket URL for terminal streaming (e.g. ws://127.0.0.1:7880)
    #[arg(long)]
    pub livekit_url: Option<String>,

    /// LiveKit JWT token for server participant
    #[arg(long)]
    pub livekit_token: Option<String>,

    /// LiveKit room name
    #[arg(long)]
    pub livekit_room: Option<String>,

    /// LiveKit participant identity (server)
    #[arg(long)]
    pub livekit_identity: Option<String>,

    /// LiveKit API key for token generation (mutually exclusive with --livekit-token)
    #[arg(long, env = "LIVEKIT_API_KEY")]
    pub livekit_api_key: Option<String>,

    /// LiveKit API secret for token generation (mutually exclusive with --livekit-token)
    #[arg(long, env = "LIVEKIT_API_SECRET")]
    pub livekit_api_secret: Option<String>,

    /// Public LiveKit URL for browser clients (e.g. ws://192.168.1.10:7880). Defaults to --livekit-url.
    #[arg(long)]
    pub livekit_public_url: Option<String>,

    /// Port for HTTP static file server (serves --web-bundle-path). Requires --web-bundle-path.
    #[arg(long, value_name = "PORT")]
    pub web_port: Option<u16>,

    /// Path to pre-built web bundle (e.g. packages/tddy-web/dist). Requires --web-port.
    #[arg(long)]
    pub web_bundle_path: Option<PathBuf>,

    /// Host to bind web server (e.g. 127.0.0.1 or 0.0.0.0). Default: 0.0.0.0 when web server is enabled.
    #[arg(long, value_name = "HOST")]
    pub web_host: Option<String>,

    /// Public URL for browser-facing redirects (e.g. http://192.168.1.10:8899)
    #[arg(long)]
    pub web_public_url: Option<String>,

    /// GitHub OAuth client ID
    #[arg(long, env = "GITHUB_CLIENT_ID")]
    pub github_client_id: Option<String>,

    /// GitHub OAuth client secret
    #[arg(long, env = "GITHUB_CLIENT_SECRET")]
    pub github_client_secret: Option<String>,

    /// GitHub OAuth redirect URI (default: http://localhost:{web_port}/auth/callback)
    #[arg(long)]
    pub github_redirect_uri: Option<String>,

    /// Use stub GitHub OAuth provider (for testing)
    #[arg(long)]
    pub github_stub: bool,

    /// Pre-register stub code:user mappings (e.g. "test-code:testuser")
    #[arg(long)]
    pub github_stub_codes: Option<String>,

    /// Enable mouse/touch mode in the TUI
    #[arg(long)]
    pub mouse: bool,

    /// Resume from an existing session (session ID). Used when spawned by tddy-daemon.
    #[arg(long, value_name = "SESSION_ID")]
    pub resume_from: Option<String>,

    /// Session ID for new daemon sessions. Used when spawned by tddy-daemon.
    #[arg(long, value_name = "SESSION_ID")]
    pub session_id: Option<String>,

    /// Project ID for daemon sessions. Used when spawned by tddy-daemon.
    #[arg(long, value_name = "PROJECT_ID")]
    pub project_id: Option<String>,
}

fn is_debug_mode(args: &Args) -> bool {
    if let Some(level) = args.log_level {
        return level >= log::LevelFilter::Debug;
    }
    args.log
        .as_ref()
        .is_some_and(|c| c.default.level >= log::LevelFilter::Debug)
}

fn effective_log_config(args: &Args) -> tddy_core::LogConfig {
    let level_override = args.log_level;
    let mut config = args
        .log
        .clone()
        .unwrap_or_else(|| tddy_core::default_log_config(level_override, None));
    if let Some(level) = level_override {
        config.default.level = level;
    }
    config
}

fn parse_log_level(s: Option<&str>) -> Option<log::LevelFilter> {
    s.and_then(|s| {
        s.parse::<log::LevelFilter>()
            .ok()
            .or_else(|| match s.to_lowercase().as_str() {
                "off" => Some(log::LevelFilter::Off),
                "error" => Some(log::LevelFilter::Error),
                "warn" => Some(log::LevelFilter::Warn),
                "info" => Some(log::LevelFilter::Info),
                "debug" => Some(log::LevelFilter::Debug),
                "trace" => Some(log::LevelFilter::Trace),
                _ => None,
            })
    })
}

impl From<CoderArgs> for Args {
    fn from(a: CoderArgs) -> Args {
        Args {
            goal: a.goal,
            plan_dir: a.plan_dir,
            conversation_output: a.conversation_output,
            model: a.model,
            allowed_tools: a.allowed_tools,
            log: None,
            log_level: parse_log_level(a.log_level.as_deref()),
            agent: a.agent,
            prompt: a.prompt,
            grpc: a.grpc,
            session_id: a.session_id,
            resume_from: a.resume_from,
            daemon: a.daemon,
            livekit_url: a.livekit_url,
            livekit_token: a.livekit_token,
            livekit_room: a.livekit_room,
            livekit_identity: a.livekit_identity,
            livekit_api_key: a.livekit_api_key,
            livekit_api_secret: a.livekit_api_secret,
            livekit_public_url: a.livekit_public_url,
            web_port: a.web_port,
            web_bundle_path: a.web_bundle_path.clone(),
            web_host: a.web_host.clone(),
            web_public_url: a.web_public_url,
            github_client_id: a.github_client_id,
            github_client_secret: a.github_client_secret,
            github_redirect_uri: a.github_redirect_uri,
            github_stub: a.github_stub,
            github_stub_codes: a.github_stub_codes,
            mouse: a.mouse,
            project_id: a.project_id,
            cursor_agent_path: a.cursor_agent_path,
        }
    }
}

impl From<DemoArgs> for Args {
    fn from(a: DemoArgs) -> Args {
        Args {
            goal: a.goal,
            plan_dir: a.plan_dir,
            conversation_output: a.conversation_output,
            model: a.model,
            allowed_tools: a.allowed_tools,
            log: None,
            log_level: parse_log_level(a.log_level.as_deref()),
            agent: a.agent.or(Some("stub".to_string())),
            prompt: a.prompt,
            grpc: a.grpc,
            session_id: a.session_id,
            resume_from: a.resume_from,
            daemon: a.daemon,
            livekit_url: a.livekit_url,
            livekit_token: a.livekit_token,
            livekit_room: a.livekit_room,
            livekit_identity: a.livekit_identity,
            livekit_api_key: a.livekit_api_key,
            livekit_api_secret: a.livekit_api_secret,
            livekit_public_url: a.livekit_public_url,
            web_port: a.web_port,
            web_bundle_path: a.web_bundle_path.clone(),
            web_host: a.web_host.clone(),
            web_public_url: a.web_public_url,
            github_client_id: a.github_client_id,
            github_client_secret: a.github_client_secret,
            github_redirect_uri: a.github_redirect_uri,
            github_stub: a.github_stub,
            github_stub_codes: a.github_stub_codes,
            mouse: a.mouse,
            project_id: a.project_id,
            cursor_agent_path: None,
        }
    }
}

/// Validate LiveKit args: mutual exclusivity of token vs key/secret, and completeness.
fn validate_livekit_args(args: &Args) -> anyhow::Result<()> {
    let has_token = args.livekit_token.is_some();
    let has_key_secret = args.livekit_api_key.is_some() && args.livekit_api_secret.is_some();

    if has_token && has_key_secret {
        anyhow::bail!(
            "--livekit-token and --livekit-api-key/--livekit-api-secret are mutually exclusive"
        );
    }

    let has_any_livekit = args.livekit_url.is_some()
        || args.livekit_token.is_some()
        || args.livekit_api_key.is_some()
        || args.livekit_api_secret.is_some()
        || args.livekit_room.is_some()
        || args.livekit_identity.is_some();

    let livekit_complete = args.livekit_url.is_some()
        && (has_token || has_key_secret)
        && args.livekit_room.is_some()
        && args.livekit_identity.is_some();

    if has_any_livekit && !livekit_complete {
        anyhow::bail!(
            "LiveKit requires all of: --livekit-url, (--livekit-token OR --livekit-api-key + --livekit-api-secret), --livekit-room, --livekit-identity"
        );
    }

    Ok(())
}

/// Validate that --web-port and --web-bundle-path are both present or both absent.
fn validate_web_args(args: &Args) -> anyhow::Result<()> {
    match (&args.web_port, &args.web_bundle_path) {
        (Some(_), None) => anyhow::bail!("--web-port requires --web-bundle-path"),
        (None, Some(_)) => anyhow::bail!("--web-bundle-path requires --web-port"),
        _ => Ok(()),
    }
}

/// Build an optional AuthService RPC entry based on CLI args.
fn build_auth_service_entry(args: &Args) -> Option<tddy_rpc::ServiceEntry> {
    if args.github_stub {
        let client_id = args.github_client_id.as_deref().unwrap_or("stub-client-id");
        // In stub mode with a web server, return a callback URL on the same origin
        // so the browser stays on the same domain (no cross-origin redirect to github.com).
        // Use web_public_url if set, otherwise derive from host+port.
        let stub = if let Some(ref public_url) = args.web_public_url {
            let callback_url = format!("{}/auth/callback", public_url.trim_end_matches('/'));
            tddy_github::StubGitHubProvider::new_with_callback(&callback_url, client_id)
        } else if let Some(port) = args.web_port {
            let host = args.web_host.as_deref().unwrap_or("127.0.0.1");
            let callback_url = format!("http://{}:{}/auth/callback", host, port);
            tddy_github::StubGitHubProvider::new_with_callback(&callback_url, client_id)
        } else {
            tddy_github::StubGitHubProvider::new("https://github.com", client_id)
        };
        if let Some(ref codes) = args.github_stub_codes {
            for mapping in codes.split(',') {
                let parts: Vec<&str> = mapping.splitn(2, ':').collect();
                if parts.len() == 2 {
                    stub.register_code(
                        parts[0],
                        tddy_github::GitHubUser {
                            id: 1,
                            login: parts[1].to_string(),
                            avatar_url: format!("https://github.com/{}.png", parts[1]),
                            name: parts[1].to_string(),
                        },
                    );
                }
            }
        }
        let auth_service_impl = tddy_github::AuthServiceImpl::new(stub);
        let auth_server = tddy_service::AuthServiceServer::new(auth_service_impl);
        Some(tddy_rpc::ServiceEntry {
            name: "auth.AuthService",
            service: std::sync::Arc::new(auth_server) as std::sync::Arc<dyn tddy_rpc::RpcService>,
        })
    } else if let (Some(id), Some(secret)) = (&args.github_client_id, &args.github_client_secret) {
        let redirect_uri = args.github_redirect_uri.clone().unwrap_or_else(|| {
            if let Some(ref public_url) = args.web_public_url {
                format!("{}/auth/callback", public_url.trim_end_matches('/'))
            } else {
                let port = args.web_port.unwrap_or(8080);
                format!("http://localhost:{}/auth/callback", port)
            }
        });
        let real = tddy_github::RealGitHubProvider::new(id, secret, &redirect_uri);
        let auth_service_impl = tddy_github::AuthServiceImpl::new(real);
        let auth_server = tddy_service::AuthServiceServer::new(auth_service_impl);
        Some(tddy_rpc::ServiceEntry {
            name: "auth.AuthService",
            service: std::sync::Arc::new(auth_server) as std::sync::Arc<dyn tddy_rpc::RpcService>,
        })
    } else {
        None
    }
}

/// Build client config for the web frontend (served at /api/config).
fn build_client_config(args: &Args) -> crate::web_server::ClientConfig {
    crate::web_server::ClientConfig {
        livekit_url: args
            .livekit_public_url
            .clone()
            .or_else(|| args.livekit_url.clone()),
        livekit_room: args.livekit_room.clone(),
        daemon_mode: None,
    }
}

/// Main entry point. Run the workflow with the given args.
pub fn run_with_args(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    validate_web_args(args)?;
    validate_livekit_args(args)?;
    if let Some(ref a) = args.agent {
        verify_tddy_tools_available(a)?;
    }
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

    let resolved_agent = match &args.agent {
        Some(a) => a.clone(),
        None => resolve_agent_for_single_goal_plain(args)?,
    };

    log::debug!(
        "[tddy-coder] goal: {}, agent: {}, model: {}",
        args.goal.as_deref().unwrap_or("(none)"),
        resolved_agent,
        args.model.as_deref().unwrap_or("(default)")
    );

    let backend = create_backend(
        &resolved_agent,
        args.cursor_agent_path.as_deref(),
        None,
        None,
    );

    if args.goal.as_deref() == Some("acceptance-tests") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for acceptance-tests goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "acceptance-tests", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("green") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for green goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, &resolved_agent, |c| {
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
        let ctx = build_goal_context(args, Some(plan_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "evaluate", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("demo") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for demo goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "demo", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("red") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for red goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "red", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("validate") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for validate goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "validate", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("refactor") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for refactor goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "refactor", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("update-docs") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for update-docs goal"))?;
        let conv = resolve_log_defaults(args, plan_dir);
        let ctx = build_goal_context(args, Some(plan_dir), &conv, &resolved_agent, |_| {});
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

    let base = tddy_core::output::sessions_base_path().map_err(|e| anyhow::anyhow!("{}", e))?;
    let plan_dir = if let Some(ref sid) = args.session_id {
        tddy_core::output::create_session_dir_with_id(&base, sid)
    } else {
        tddy_core::output::create_session_dir_in(&base)
    }
    .context("create session dir")?;
    let output_dir_for_ctx =
        std::env::current_dir().context("current dir for agent working_dir")?;

    let conv = resolve_log_defaults(args, &plan_dir);
    let ctx = build_goal_context(args, None, &conv, &resolved_agent, |c| {
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
    // Logger is already initialized in `run_main` with `effective_log_config(args)`.
    // Do not call `init_tddy_logger` again: `log::set_logger` only succeeds once; a second
    // init would skip `set_max_level` and can leave FILE_OUTPUTS / routing inconsistent.

    let sessions_base = tddy_core::output::sessions_base_path()
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .join(tddy_core::output::SESSIONS_SUBDIR);
    std::fs::create_dir_all(&sessions_base).context("create sessions base dir")?;

    let port = args.grpc.unwrap_or(50051);
    let agent_str = args.agent.as_deref().unwrap_or("claude");
    if args.agent.is_none() {
        verify_tddy_tools_available(agent_str)?;
    }
    let backend = create_backend(agent_str, args.cursor_agent_path.as_deref(), None, None);
    let has_token = args.livekit_token.is_some();
    let has_key_secret = args.livekit_api_key.is_some() && args.livekit_api_secret.is_some();
    let livekit_enabled = args.livekit_url.is_some()
        && (has_token || has_key_secret)
        && args.livekit_room.is_some()
        && args.livekit_identity.is_some();

    let service = tddy_service::DaemonService::new(sessions_base.clone(), backend.clone());
    let view_factory: Option<Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync>> =
        if livekit_enabled {
            let (event_tx, _) = tokio::sync::broadcast::channel(256);
            let (intent_tx, intent_rx) = std::sync::mpsc::channel();
            let mut presenter = Presenter::new(
                agent_str,
                args.model
                    .as_deref()
                    .unwrap_or_else(|| default_model_for_agent(agent_str)),
            )
            .with_broadcast(event_tx)
            .with_intent_sender(intent_tx);
            let output_dir = args
                .resume_from
                .as_deref()
                .or(args.session_id.as_deref())
                .map(|id| sessions_base.join(id))
                .unwrap_or_else(|| sessions_base.join("tddy-daemon-session"));
            let _ = std::fs::create_dir_all(&output_dir);
            let logs = output_dir.join("logs");
            let _ = std::fs::create_dir_all(&logs);
            tddy_core::toolcall::set_toolcall_log_dir(&logs);

            let (toolcall_socket_path, tool_call_rx) =
                match tddy_core::toolcall::start_toolcall_listener() {
                    Ok((path, rx)) => (Some(path), Some(rx)),
                    Err(_) => (None, None),
                };

            let now = chrono::Utc::now().to_rfc3339();
            let session_id = args
                .resume_from
                .as_deref()
                .or(args.session_id.as_deref())
                .unwrap_or("tddy-daemon-session");
            let session_metadata = tddy_core::SessionMetadata {
                session_id: session_id.to_string(),
                project_id: args.project_id.clone().unwrap_or_default(),
                created_at: now.clone(),
                updated_at: now,
                status: "active".to_string(),
                repo_path: std::env::current_dir()
                    .ok()
                    .map(|p| p.display().to_string()),
                pid: Some(std::process::id()),
                tool: Some("tddy-coder".to_string()),
                livekit_room: args.livekit_room.clone(),
            };
            let _ = tddy_core::write_session_metadata(&output_dir, &session_metadata);
            // New daemon sessions must not use a placeholder prompt: stdin is /dev/null from the
            // parent spawner, so the workflow must block on `answer_rx` until the user submits
            // feature text via Virtual TUI / LiveKit (SubmitFeatureInput). A placeholder skips
            // that and jumps straight into plan / first clarification.
            let (plan_dir, initial_prompt) = if args.resume_from.is_some() {
                (Some(output_dir.clone()), None)
            } else {
                (None, None)
            };
            presenter.start_workflow(
                backend,
                output_dir,
                plan_dir,
                initial_prompt,
                None,
                None,
                false,
                None,
                toolcall_socket_path,
                tool_call_rx,
            );
            let presenter = Arc::new(Mutex::new(presenter));
            let presenter_for_factory = presenter.clone();
            let factory: Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync> =
                Arc::new(move || {
                    presenter_for_factory
                        .lock()
                        .ok()
                        .and_then(|p| p.connect_view())
                });
            let shutdown_for_thread = shutdown.clone();
            let presenter_for_thread = presenter.clone();
            std::thread::spawn(move || {
                for _ in 0..100_000 {
                    if shutdown_for_thread.load(Ordering::Relaxed) {
                        break;
                    }
                    while let Ok(intent) = intent_rx.try_recv() {
                        if let Ok(mut p) = presenter_for_thread.lock() {
                            p.handle_intent(intent);
                        }
                    }
                    if let Ok(mut p) = presenter_for_thread.lock() {
                        p.poll_tool_calls();
                        p.poll_workflow();
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            });
            Some(factory)
        } else {
            None
        };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("create tokio runtime")?;

    rt.block_on(async {
        let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .context("bind gRPC port")?;
        log::info!("tddy-coder daemon listening on port {}", port);

        let mut grpc_router = tonic::transport::Server::builder()
            .add_service(tddy_service::gen::tddy_remote_server::TddyRemoteServer::new(service));
        if let Some(ref factory) = view_factory {
            grpc_router = grpc_router.add_service(
                tddy_service::tonic_terminal::terminal_service_server::TerminalServiceServer::new(
                    tddy_service::TerminalServiceVirtualTui::new(factory.clone(), args.mouse),
                ),
            );
        }
        let server = grpc_router
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener));

        if let (Some(web_port), Some(ref web_bundle_path)) = (args.web_port, &args.web_bundle_path)
        {
            let path = web_bundle_path.clone();
            let web_host = args.web_host.as_deref().unwrap_or("0.0.0.0").to_string();
            let rpc_router = if has_key_secret {
                let token_generator = std::sync::Arc::new(tddy_livekit::TokenGenerator::new(
                    args.livekit_api_key.as_ref().unwrap().clone(),
                    args.livekit_api_secret.as_ref().unwrap().clone(),
                    args.livekit_room.as_ref().unwrap().clone(),
                    args.livekit_identity.as_ref().unwrap().clone(),
                    std::time::Duration::from_secs(120),
                ));
                let token_provider = LiveKitTokenProvider(token_generator);
                let token_service_impl = tddy_service::TokenServiceImpl::new(token_provider);
                let token_server = tddy_service::TokenServiceServer::new(token_service_impl);
                let echo_server =
                    tddy_service::EchoServiceServer::new(tddy_service::EchoServiceImpl);
                let mut entries = vec![
                    tddy_rpc::ServiceEntry {
                        name: "test.EchoService",
                        service: std::sync::Arc::new(echo_server)
                            as std::sync::Arc<dyn tddy_rpc::RpcService>,
                    },
                    tddy_rpc::ServiceEntry {
                        name: "token.TokenService",
                        service: std::sync::Arc::new(token_server)
                            as std::sync::Arc<dyn tddy_rpc::RpcService>,
                    },
                ];
                if let Some(auth_entry) = build_auth_service_entry(args) {
                    entries.push(auth_entry);
                }
                let multi = tddy_rpc::MultiRpcService::new(entries);
                Some(tddy_connectrpc::connect_router(tddy_rpc::RpcBridge::new(
                    multi,
                )))
            } else {
                let echo_server =
                    tddy_service::EchoServiceServer::new(tddy_service::EchoServiceImpl);
                let mut entries = vec![tddy_rpc::ServiceEntry {
                    name: "test.EchoService",
                    service: std::sync::Arc::new(echo_server)
                        as std::sync::Arc<dyn tddy_rpc::RpcService>,
                }];
                if let Some(auth_entry) = build_auth_service_entry(args) {
                    entries.push(auth_entry);
                }
                let multi = tddy_rpc::MultiRpcService::new(entries);
                Some(tddy_connectrpc::connect_router(tddy_rpc::RpcBridge::new(
                    multi,
                )))
            };
            let client_config = build_client_config(args);
            tokio::spawn(async move {
                if let Err(e) = crate::web_server::serve_web_bundle(
                    &web_host,
                    web_port,
                    path,
                    rpc_router,
                    Some(client_config),
                )
                .await
                {
                    log::error!("Web server error: {}", e);
                }
            });
        }

        if livekit_enabled {
            let url = args.livekit_url.as_ref().unwrap().clone();
            let shutdown_clone = shutdown.clone();
            let factory = view_factory
                .clone()
                .expect("factory set when livekit_enabled");
            let terminal_service =
                tddy_service::TerminalServiceVirtualTui::new(factory, args.mouse);
            if has_key_secret {
                let token_generator = tddy_livekit::TokenGenerator::new(
                    args.livekit_api_key.as_ref().unwrap().clone(),
                    args.livekit_api_secret.as_ref().unwrap().clone(),
                    args.livekit_room.as_ref().unwrap().clone(),
                    args.livekit_identity.as_ref().unwrap().clone(),
                    std::time::Duration::from_secs(120),
                );
                tokio::spawn(async move {
                    tddy_livekit::LiveKitParticipant::run_with_reconnect(
                        &url,
                        &token_generator,
                        tddy_service::TerminalServiceServer::new(terminal_service),
                        tddy_livekit::RoomOptions::default(),
                        shutdown_clone,
                    )
                    .await
                });
            } else {
                let token = args.livekit_token.as_ref().unwrap().clone();
                tokio::spawn(async move {
                    let participant = match tddy_livekit::LiveKitParticipant::connect(
                        &url,
                        &token,
                        tddy_service::TerminalServiceServer::new(terminal_service),
                        tddy_livekit::RoomOptions::default(),
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
    let base = tddy_core::output::sessions_base_path().ok()?;
    Some(base.join(tddy_core::output::SESSIONS_SUBDIR).join(sid))
}

/// When [`Args::resume_from`] is set and `plan_dir` is unset, sets `plan_dir` to the session directory (`session_dir_path`).
fn apply_plan_dir_from_session_if_needed(args: &mut Args) -> anyhow::Result<()> {
    if args.plan_dir.is_some() {
        return Ok(());
    }
    if args.resume_from.is_none() {
        return Ok(());
    }
    if let Some(dir) = session_dir_path(args) {
        args.plan_dir = Some(dir);
    }
    Ok(())
}

/// Merges [`crate::config::SESSION_CODER_CONFIG_FILE`] from the session directory when
/// [`Args::resume_from`] is set. Uses the same YAML schema and merge rules as `-c` / [`crate::config::merge_config_into_args`].
pub fn merge_session_coder_config_for_resume(args: &mut Args) -> anyhow::Result<()> {
    let Some(ref sid) = args.resume_from else {
        return Ok(());
    };
    let base = tddy_core::output::sessions_base_path().map_err(|e| anyhow::anyhow!("{}", e))?;
    let dir = base.join(tddy_core::output::SESSIONS_SUBDIR).join(sid);
    merge_session_coder_config_from_dir(args, &dir)
}

fn merge_session_coder_config_from_dir(
    args: &mut Args,
    session_plan_dir: &Path,
) -> anyhow::Result<()> {
    let path = session_plan_dir.join(crate::config::SESSION_CODER_CONFIG_FILE);
    if !path.is_file() {
        return Ok(());
    }
    let config = crate::config::load_config(&path)?;
    crate::config::merge_config_into_args(args, config);
    Ok(())
}

/// When `plan_dir` has `changeset.yaml` with session entries, sets `agent` from
/// [`tddy_core::resolve_agent_from_changeset`] if the CLI left the default `claude`.
fn apply_agent_from_changeset_if_needed(args: &mut Args) -> anyhow::Result<()> {
    if args.agent.as_deref().is_some_and(|a| a != "claude") {
        return Ok(());
    }
    let Some(ref plan_dir) = args.plan_dir else {
        return Ok(());
    };
    let cs = match tddy_core::read_changeset(plan_dir) {
        Ok(cs) => cs,
        Err(_) => return Ok(()),
    };
    if let Some(agent) = tddy_core::resolve_agent_from_changeset(&cs) {
        args.agent = Some(agent);
    }
    Ok(())
}

/// Resolve Cursor `agent` executable: `--cursor-agent-path` / config, then `TDDY_CURSOR_AGENT`, then `agent` on `PATH`.
fn resolve_cursor_agent_binary(cursor_agent_path: Option<&Path>) -> PathBuf {
    cursor_agent_path
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TDDY_CURSOR_AGENT").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(CursorBackend::DEFAULT_CLI_BINARY))
}

/// Create backend once at startup (plain mode, no progress events).
/// StubBackend always uses InMemoryToolExecutor (no tddy-tools): stub simulates the agent,
/// so it stores results directly. ProcessToolExecutor is for real agents (Claude/Cursor)
/// that run tddy-tools submit.
fn create_backend(
    agent: &str,
    cursor_agent_path: Option<&Path>,
    _socket_path: Option<&Path>,
    _working_dir: Option<&Path>,
) -> SharedBackend {
    log::info!("[tddy-coder] using agent: {}", agent);
    let backend: AnyBackend = match agent {
        "cursor" => {
            let path = resolve_cursor_agent_binary(cursor_agent_path);
            log::info!(
                "[tddy-coder] Cursor backend: CLI binary `{}` (full argv logged when a goal invokes the backend)",
                path.display()
            );
            AnyBackend::Cursor(CursorBackend::with_path(path).with_progress(on_progress))
        }
        "claude-acp" => AnyBackend::ClaudeAcp(ClaudeAcpBackend::new()),
        "stub" => AnyBackend::Stub(StubBackend::new()),
        _ => AnyBackend::Claude(ClaudeCodeBackend::new().with_progress(on_progress)),
    };
    SharedBackend::from_any(backend)
}

/// Resolve conversation_output and debug_output defaults to plan_dir/logs/ when not set.
/// Returns the resolved conversation output path for use in context.
fn resolve_log_defaults(args: &Args, plan_dir: &Path) -> Option<PathBuf> {
    tddy_core::resolve_log_defaults(args.conversation_output.clone(), None::<&Path>, plan_dir)
}

/// Build context_values for a goal from args and plan_dir.
fn build_goal_context(
    args: &Args,
    plan_dir: Option<&PathBuf>,
    conversation_output: &Option<PathBuf>,
    resolved_agent_for_model: &str,
    extra: impl FnOnce(&mut std::collections::HashMap<String, serde_json::Value>),
) -> std::collections::HashMap<String, serde_json::Value> {
    let inherit_stdin = io::stdin().is_terminal();
    let mut ctx = std::collections::HashMap::new();
    let model_val = args
        .model
        .clone()
        .unwrap_or_else(|| default_model_for_agent(resolved_agent_for_model).to_string());
    ctx.insert(
        "model".to_string(),
        serde_json::to_value(model_val).unwrap(),
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
    ctx.insert("debug".to_string(), serde_json::json!(is_debug_mode(args)));
    if let Some(p) = plan_dir {
        ctx.insert("plan_dir".to_string(), serde_json::to_value(p).unwrap());
        // Repo root is stored in changeset.repo_path (set when plan started). Use it for worktree creation.
        let output_dir = tddy_core::read_changeset(p)
            .ok()
            .and_then(|cs| cs.repo_path.map(PathBuf::from))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
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
                    .unwrap_or_else(|| args.plan_dir.clone().unwrap_or_else(|| PathBuf::from(".")));
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
                    .unwrap_or_else(|| args.plan_dir.clone().unwrap_or_else(|| PathBuf::from(".")));
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

    if let Some(session_dir) = session_dir_path(args) {
        let logs = session_dir.join("logs");
        tddy_core::toolcall::set_toolcall_log_dir(&logs);
    }

    let (socket_path, tool_call_rx) = match tddy_core::toolcall::start_toolcall_listener() {
        Ok((path, rx)) => (Some(path), Some(rx)),
        Err(_) => (None, None),
    };
    let (event_tx, _) = tokio::sync::broadcast::channel(256);
    let (intent_tx, intent_rx) = std::sync::mpsc::channel();
    let presenter = match args.agent.as_deref() {
        Some(a) => {
            let m = args
                .model
                .as_deref()
                .unwrap_or_else(|| default_model_for_agent(a));
            Presenter::new(a, m)
        }
        None => {
            let m = args
                .model
                .as_deref()
                .unwrap_or_else(|| default_model_for_agent("claude"));
            Presenter::new("claude", m)
        }
    }
    .with_broadcast(event_tx.clone())
    .with_intent_sender(intent_tx.clone());
    let presenter = Arc::new(Mutex::new(presenter));

    if args.agent.is_none() {
        let q = backend_selection_question();
        let idx = preselected_index_for_agent("claude");
        let socket_path_for_factory = socket_path.clone();
        let cursor_path_for_factory = args.cursor_agent_path.clone();
        let mut p = presenter.lock().unwrap();
        p.configure_deferred_workflow_start(
            Box::new(move |agent: &str| {
                verify_tddy_tools_available(agent).map_err(|e| e.to_string())?;
                Ok(create_backend(
                    agent,
                    cursor_path_for_factory.as_deref(),
                    socket_path_for_factory.as_deref(),
                    None,
                ))
            }),
            PendingWorkflowStart {
                output_dir: PathBuf::from("."),
                plan_dir: args.plan_dir.clone(),
                initial_prompt: args.prompt.clone(),
                conversation_output_path: args.conversation_output.clone(),
                debug_output_path: None,
                debug: is_debug_mode(args),
                session_id: args.session_id.clone(),
                socket_path,
                tool_call_rx,
            },
            args.model.clone(),
        );
        p.show_backend_selection(q, idx);
    } else {
        let agent = args.agent.as_deref().unwrap();
        let backend = create_backend(
            agent,
            args.cursor_agent_path.as_deref(),
            socket_path.as_deref(),
            None,
        );
        presenter.lock().unwrap().start_workflow(
            backend,
            PathBuf::from("."),
            args.plan_dir.clone(),
            args.prompt.clone(),
            args.conversation_output.clone(),
            None,
            is_debug_mode(args),
            args.session_id.clone(),
            socket_path,
            tool_call_rx,
        );
    }

    let has_token = args.livekit_token.is_some();
    let has_key_secret = args.livekit_api_key.is_some() && args.livekit_api_secret.is_some();
    let livekit_enabled = args.livekit_url.is_some()
        && (has_token || has_key_secret)
        && args.livekit_room.is_some()
        && args.livekit_identity.is_some();

    let presenter_for_factory = presenter.clone();
    let view_factory: Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync> =
        Arc::new(move || {
            presenter_for_factory
                .lock()
                .ok()
                .and_then(|p| p.connect_view())
        });

    if let Some(port) = args.grpc {
        let handle = tddy_core::PresenterHandle {
            event_tx: event_tx.clone(),
            intent_tx: intent_tx.clone(),
        };
        let service = tddy_service::TddyRemoteService::new(handle);
        let terminal_svc =
            tddy_service::TerminalServiceVirtualTui::new(view_factory.clone(), args.mouse);
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
                    .add_service(
                        tddy_service::gen::tddy_remote_server::TddyRemoteServer::new(service),
                    )
                    .add_service(
                        tddy_service::tonic_terminal::terminal_service_server::TerminalServiceServer::new(terminal_svc),
                    )
                    .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                    .await
                    .map_err(anyhow::Error::from)
            });
            result.expect("gRPC server failed")
        });
    }

    if livekit_enabled {
        let terminal_service =
            tddy_service::TerminalServiceVirtualTui::new(view_factory.clone(), args.mouse);
        let url = args.livekit_url.clone().unwrap();
        let shutdown = shutdown.clone();
        if has_key_secret {
            let token_generator = std::sync::Arc::new(tddy_livekit::TokenGenerator::new(
                args.livekit_api_key.as_ref().unwrap().clone(),
                args.livekit_api_secret.as_ref().unwrap().clone(),
                args.livekit_room.as_ref().unwrap().clone(),
                args.livekit_identity.as_ref().unwrap().clone(),
                std::time::Duration::from_secs(120),
            ));
            let token_provider = LiveKitTokenProvider(token_generator.clone());
            let token_service_impl = tddy_service::TokenServiceImpl::new(token_provider);
            let terminal_server = tddy_service::TerminalServiceServer::new(terminal_service);
            let token_server = tddy_service::TokenServiceServer::new(token_service_impl);
            let multi_service = tddy_rpc::MultiRpcService::new(vec![
                tddy_rpc::ServiceEntry {
                    name: "terminal.TerminalService",
                    service: std::sync::Arc::new(terminal_server)
                        as std::sync::Arc<dyn tddy_rpc::RpcService>,
                },
                tddy_rpc::ServiceEntry {
                    name: "token.TokenService",
                    service: std::sync::Arc::new(token_server)
                        as std::sync::Arc<dyn tddy_rpc::RpcService>,
                },
            ]);
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(async {
                    tddy_livekit::LiveKitParticipant::run_with_reconnect(
                        &url,
                        token_generator.as_ref(),
                        multi_service,
                        tddy_livekit::RoomOptions::default(),
                        shutdown,
                    )
                    .await
                });
            });
        } else {
            let token = args.livekit_token.clone().unwrap();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(async {
                    match tddy_livekit::LiveKitParticipant::connect(
                        &url,
                        &token,
                        tddy_service::TerminalServiceServer::new(terminal_service),
                        tddy_livekit::RoomOptions::default(),
                    )
                    .await
                    {
                        Ok(participant) => {
                            log::info!("READY");
                            participant.run().await;
                        }
                        Err(e) => {
                            log::error!("LiveKit connect failed: {}", e);
                        }
                    }
                });
            });
        }
    }

    if let (Some(web_port), Some(ref web_bundle_path)) = (args.web_port, &args.web_bundle_path) {
        let path = web_bundle_path.clone();
        let web_host = args.web_host.as_deref().unwrap_or("0.0.0.0").to_string();
        let rpc_router = if has_key_secret {
            let token_generator = std::sync::Arc::new(tddy_livekit::TokenGenerator::new(
                args.livekit_api_key.as_ref().unwrap().clone(),
                args.livekit_api_secret.as_ref().unwrap().clone(),
                args.livekit_room.as_ref().unwrap().clone(),
                args.livekit_identity.as_ref().unwrap().clone(),
                std::time::Duration::from_secs(120),
            ));
            let token_provider = LiveKitTokenProvider(token_generator);
            let token_service_impl = tddy_service::TokenServiceImpl::new(token_provider);
            let token_server = tddy_service::TokenServiceServer::new(token_service_impl);
            let echo_server = tddy_service::EchoServiceServer::new(tddy_service::EchoServiceImpl);
            let mut entries = vec![
                tddy_rpc::ServiceEntry {
                    name: "test.EchoService",
                    service: std::sync::Arc::new(echo_server)
                        as std::sync::Arc<dyn tddy_rpc::RpcService>,
                },
                tddy_rpc::ServiceEntry {
                    name: "token.TokenService",
                    service: std::sync::Arc::new(token_server)
                        as std::sync::Arc<dyn tddy_rpc::RpcService>,
                },
            ];
            if let Some(auth_entry) = build_auth_service_entry(args) {
                entries.push(auth_entry);
            }
            let multi = tddy_rpc::MultiRpcService::new(entries);
            Some(tddy_connectrpc::connect_router(tddy_rpc::RpcBridge::new(
                multi,
            )))
        } else {
            let echo_server = tddy_service::EchoServiceServer::new(tddy_service::EchoServiceImpl);
            let mut entries = vec![tddy_rpc::ServiceEntry {
                name: "test.EchoService",
                service: std::sync::Arc::new(echo_server)
                    as std::sync::Arc<dyn tddy_rpc::RpcService>,
            }];
            if let Some(auth_entry) = build_auth_service_entry(args) {
                entries.push(auth_entry);
            }
            let multi = tddy_rpc::MultiRpcService::new(entries);
            Some(tddy_connectrpc::connect_router(tddy_rpc::RpcBridge::new(
                multi,
            )))
        };
        let client_config = build_client_config(args);
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime for web server");
            if let Err(e) = rt.block_on(crate::web_server::serve_web_bundle(
                &web_host,
                web_port,
                path,
                rpc_router,
                Some(client_config),
            )) {
                log::error!("Web server error: {}", e);
            }
        });
    }

    let conn = presenter
        .lock()
        .unwrap()
        .connect_view()
        .expect("connect_view requires broadcast and intent_tx");
    let shutdown_for_thread = shutdown.clone();
    let presenter_for_thread = presenter.clone();
    let presenter_handle = std::thread::spawn(move || {
        for _ in 0..100_000 {
            if shutdown_for_thread.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(intent) = intent_rx.try_recv() {
                if let Ok(mut p) = presenter_for_thread.lock() {
                    p.handle_intent(intent);
                }
            }
            if let Ok(mut p) = presenter_for_thread.lock() {
                p.poll_tool_calls();
                p.poll_workflow();
                if p.state().should_quit {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    tddy_tui::run_event_loop(
        conn,
        shutdown.as_ref(),
        None,
        is_debug_mode(args),
        args.mouse,
    )?;

    presenter_handle.join().expect("presenter thread panicked");

    // If user chose "Continue with agent", exec into claude --resume <session_id>.
    if let Some(tddy_core::ExitAction::ContinueWithAgent { ref session_id }) =
        presenter.lock().unwrap().state().exit_action
    {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = std::process::Command::new("claude")
                .arg("--resume")
                .arg(session_id)
                .exec();
            eprintln!("Failed to exec claude: {}", err);
            std::process::exit(1);
        }
        #[cfg(not(unix))]
        {
            eprintln!("Continue with agent is only supported on Unix platforms");
            std::process::exit(1);
        }
    }

    if let Some(result) = presenter.lock().unwrap().take_workflow_result() {
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
    let agent_str = resolve_agent_for_full_workflow_plain(args)?;
    let backend = create_backend(
        &agent_str,
        args.cursor_agent_path.as_deref(),
        None,
        None,
    );

    let mut plan_dir = if let Some(ref p) = args.plan_dir {
        p.clone()
    } else {
        run_plan_to_get_dir(args, backend.clone(), &agent_str, &shutdown)?
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
            plan_dir = run_plan_to_complete(
                args,
                backend.clone(),
                &input,
                &plan_dir,
                &agent_str,
                &shutdown,
            )?;
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
    // output_dir comes from build_goal_context (repo_path in changeset); do not overwrite with plan_dir.parent()
    // — plan_dir under ~/.tddy/sessions/ would make parent wrong for worktree creation.
    let context_values = build_goal_context(args, Some(&plan_dir), &conv, &agent_str, |c| {
        c.insert(
            "feature_input".to_string(),
            serde_json::json!(feature_input),
        );
        c.insert("run_demo".to_string(), serde_json::json!(run_demo));
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
    resolved_agent_for_model: &str,
    shutdown: &AtomicBool,
) -> anyhow::Result<PathBuf> {
    let input = read_feature_input(args).context("read feature description")?;
    let input = input.trim().to_string();
    if input.is_empty() {
        anyhow::bail!("empty feature description");
    }
    let base = tddy_core::output::sessions_base_path().map_err(|e| anyhow::anyhow!("{}", e))?;
    let plan_dir = if let Some(ref sid) = args.session_id {
        tddy_core::output::create_session_dir_with_id(&base, sid)
    } else {
        tddy_core::output::create_session_dir_in(&base)
    }
    .context("create session dir")?;
    let output_dir_for_ctx =
        std::env::current_dir().context("current dir for agent working_dir")?;
    let init_cs = tddy_core::changeset::Changeset {
        initial_prompt: Some(input.clone()),
        repo_path: Some(output_dir_for_ctx.display().to_string()),
        ..tddy_core::changeset::Changeset::default()
    };
    let _ = tddy_core::changeset::write_changeset(&plan_dir, &init_cs);

    let conv = resolve_log_defaults(args, &plan_dir);
    let ctx = build_goal_context(args, None, &conv, resolved_agent_for_model, |c| {
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
    resolved_agent_for_model: &str,
    shutdown: &AtomicBool,
) -> anyhow::Result<PathBuf> {
    // output_dir from build_goal_context (repo_path in changeset); plan_dir.parent() wrong when under ~/.tddy/sessions/
    let conv = resolve_log_defaults(args, plan_dir);
    let ctx = build_goal_context(args, Some(plan_dir), &conv, resolved_agent_for_model, |c| {
        c.insert("feature_input".to_string(), serde_json::json!(input));
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
    // output_dir from build_goal_context (repo_path in changeset); plan_dir.parent() wrong when under ~/.tddy/sessions/
    let refine_storage = std::env::temp_dir().join("tddy-flowrunner-refine-session");
    std::fs::create_dir_all(&refine_storage).context("create refine session dir")?;
    let refine_hooks = std::sync::Arc::new(tddy_core::workflow::tdd_hooks::TddWorkflowHooks::new());
    let refine_engine = WorkflowEngine::new(backend.clone(), refine_storage, Some(refine_hooks));
    let plan_dir_buf = plan_dir.to_path_buf();
    let conv = resolve_log_defaults(args, &plan_dir_buf);
    let mut refine_ctx =
        build_goal_context(args, Some(&plan_dir_buf), &conv, backend.name(), |c| {
            c.insert(
                "feature_input".to_string(),
                serde_json::json!(feature_input),
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

#[cfg(test)]
mod resume_session_config_tests {
    use super::merge_session_coder_config_for_resume;
    use super::Args;
    use serial_test::serial;

    #[test]
    #[serial]
    fn resume_from_merges_coder_config_from_session_dir() {
        let tmp =
            std::env::temp_dir().join(format!("tddy-resume-config-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create temp sessions base");
        std::env::set_var(tddy_core::output::TDDY_SESSIONS_DIR_ENV, &tmp);

        let sid = "019d105b-ac0f-78d3-9a89-409731145a36";
        let session_dir = tmp.join("sessions").join(sid);
        std::fs::create_dir_all(&session_dir).expect("create session dir");
        std::fs::write(
            session_dir.join(crate::config::SESSION_CODER_CONFIG_FILE),
            "agent: cursor\ncursor_agent_path: /persisted/cursor-agent\n",
        )
        .expect("write coder-config.yaml");

        let mut args = Args {
            goal: None,
            plan_dir: None,
            conversation_output: None,
            model: None,
            allowed_tools: None,
            log: None,
            log_level: None,
            agent: None,
            prompt: None,
            grpc: None,
            session_id: None,
            resume_from: Some(sid.to_string()),
            daemon: false,
            livekit_url: None,
            livekit_token: None,
            livekit_room: None,
            livekit_identity: None,
            livekit_api_key: None,
            livekit_api_secret: None,
            livekit_public_url: None,
            web_port: None,
            web_bundle_path: None,
            web_host: None,
            web_public_url: None,
            github_client_id: None,
            github_client_secret: None,
            github_redirect_uri: None,
            github_stub: false,
            github_stub_codes: None,
            mouse: false,
            project_id: None,
            cursor_agent_path: None,
        };

        merge_session_coder_config_for_resume(&mut args).expect("merge");

        assert_eq!(args.agent.as_deref(), Some("cursor"));
        assert_eq!(
            args.cursor_agent_path.as_ref().map(|p| p.as_path()),
            Some(std::path::Path::new("/persisted/cursor-agent"))
        );

        std::env::remove_var(tddy_core::output::TDDY_SESSIONS_DIR_ENV);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[cfg(test)]
mod resume_session_identity_tests {
    use super::assign_default_session_id;
    use super::Args;

    #[test]
    fn resume_from_sets_session_id_when_session_id_absent() {
        let sid = "019d105b-ac0f-78d3-9a89-409731145a36";
        let mut args = Args {
            goal: None,
            plan_dir: None,
            conversation_output: None,
            model: None,
            allowed_tools: None,
            log: None,
            log_level: None,
            agent: Some("claude".to_string()),
            prompt: None,
            grpc: None,
            session_id: None,
            resume_from: Some(sid.to_string()),
            daemon: false,
            livekit_url: None,
            livekit_token: None,
            livekit_room: None,
            livekit_identity: None,
            livekit_api_key: None,
            livekit_api_secret: None,
            livekit_public_url: None,
            web_port: None,
            web_bundle_path: None,
            web_host: None,
            web_public_url: None,
            github_client_id: None,
            github_client_secret: None,
            github_redirect_uri: None,
            github_stub: false,
            github_stub_codes: None,
            mouse: false,
            project_id: None,
            cursor_agent_path: None,
        };

        assign_default_session_id(&mut args);

        assert_eq!(args.session_id.as_deref(), Some(sid));
    }
}

#[cfg(test)]
mod plan_dir_from_session_tests {
    use super::apply_plan_dir_from_session_if_needed;
    use super::Args;
    use serial_test::serial;

    #[test]
    #[serial]
    fn plan_dir_derived_from_session_id_when_unset() {
        let tmp =
            std::env::temp_dir().join(format!("tddy-plan-dir-session-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create temp sessions base");
        std::env::set_var(tddy_core::output::TDDY_SESSIONS_DIR_ENV, &tmp);

        let sid = "019d105b-ac0f-78d3-9a89-409731145a36";
        let expected = tmp.join("sessions").join(sid);

        let mut args = Args {
            goal: None,
            plan_dir: None,
            conversation_output: None,
            model: None,
            allowed_tools: None,
            log: None,
            log_level: None,
            agent: Some("claude".to_string()),
            prompt: None,
            grpc: None,
            session_id: Some(sid.to_string()),
            resume_from: Some(sid.to_string()),
            daemon: false,
            livekit_url: None,
            livekit_token: None,
            livekit_room: None,
            livekit_identity: None,
            livekit_api_key: None,
            livekit_api_secret: None,
            livekit_public_url: None,
            web_port: None,
            web_bundle_path: None,
            web_host: None,
            web_public_url: None,
            github_client_id: None,
            github_client_secret: None,
            github_redirect_uri: None,
            github_stub: false,
            github_stub_codes: None,
            mouse: false,
            project_id: None,
            cursor_agent_path: None,
        };

        apply_plan_dir_from_session_if_needed(&mut args).expect("apply");

        assert_eq!(args.plan_dir, Some(expected));

        std::env::remove_var(tddy_core::output::TDDY_SESSIONS_DIR_ENV);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[cfg(test)]
mod changeset_agent_resume_tests {
    use super::apply_agent_from_changeset_if_needed;
    use super::Args;
    use serial_test::serial;
    use tddy_core::changeset::{append_session_and_update_state, write_changeset, Changeset};

    /// Backend `agent` should follow the plan session recorded in `changeset.yaml` on resume.
    #[test]
    #[serial]
    fn resume_applies_agent_from_changeset_plan_session() {
        let plan_dir =
            std::env::temp_dir().join(format!("tddy-changeset-agent-tests-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&plan_dir);
        std::fs::create_dir_all(&plan_dir).expect("plan dir");

        let mut cs = Changeset::default();
        append_session_and_update_state(
            &mut cs,
            "plan-sess".into(),
            "plan",
            "Planned",
            "cursor",
            None,
        );
        write_changeset(&plan_dir, &cs).expect("write changeset");

        let sid = "019d105b-ac0f-78d3-9a89-409731145a36";
        let mut args = Args {
            goal: None,
            plan_dir: Some(plan_dir.clone()),
            conversation_output: None,
            model: None,
            allowed_tools: None,
            log: None,
            log_level: None,
            agent: Some("claude".to_string()),
            prompt: None,
            grpc: None,
            session_id: Some(sid.to_string()),
            resume_from: Some(sid.to_string()),
            daemon: false,
            livekit_url: None,
            livekit_token: None,
            livekit_room: None,
            livekit_identity: None,
            livekit_api_key: None,
            livekit_api_secret: None,
            livekit_public_url: None,
            web_port: None,
            web_bundle_path: None,
            web_host: None,
            web_public_url: None,
            github_client_id: None,
            github_client_secret: None,
            github_redirect_uri: None,
            github_stub: false,
            github_stub_codes: None,
            mouse: false,
            project_id: None,
            cursor_agent_path: None,
        };

        apply_agent_from_changeset_if_needed(&mut args).expect("apply");

        assert_eq!(args.agent.as_deref(), Some("cursor"));

        let _ = std::fs::remove_dir_all(&plan_dir);
    }
}
