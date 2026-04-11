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
    output::SESSIONS_SUBDIR, preselected_index_for_agent, read_changeset, read_session_metadata,
    AnyBackend, ClaudeAcpBackend, ClaudeCodeBackend, CodexAcpBackend, CodexBackend, CodingBackend,
    CursorBackend, GoalId, PendingWorkflowStart, ProgressEvent, SharedBackend, StubBackend,
    WorkflowEngine, WorkflowRecipe,
};
use tddy_workflow_recipes::{parse_evaluate_response, parse_refactor_response};

use crate::plain;
use crate::tty::should_run_tui;
use tddy_core::Presenter;

use crate::disable_raw_mode;

fn recipe_arc_for_args(args: &Args) -> anyhow::Result<Arc<dyn WorkflowRecipe>> {
    let name = args
        .recipe
        .as_deref()
        .unwrap_or_else(|| crate::default_unspecified_workflow_recipe_cli_name());
    crate::resolve_workflow_recipe_from_cli_name(name.trim()).map_err(|e| anyhow::anyhow!(e))
}

fn validate_recipe_cli(args: &Args) -> anyhow::Result<()> {
    let name = args
        .recipe
        .as_deref()
        .unwrap_or_else(|| crate::default_unspecified_workflow_recipe_cli_name());
    crate::resolve_workflow_recipe_from_cli_name(name.trim())
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!(e))
}

fn apply_recipe_from_changeset_if_needed(args: &mut Args) -> anyhow::Result<()> {
    if args.recipe.is_some() {
        return Ok(());
    }
    let Some(ref session_dir) = args.session_dir else {
        return Ok(());
    };
    let cs = match tddy_core::read_changeset(session_dir) {
        Ok(cs) => cs,
        Err(_) => return Ok(()),
    };
    if let Some(r) = cs.recipe.filter(|s| !s.trim().is_empty()) {
        log::info!(
            "apply_recipe_from_changeset_if_needed: recipe {:?} from changeset.yaml",
            r
        );
        args.recipe = Some(r);
    }
    Ok(())
}

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

/// Virtual terminal + in-memory Codex OAuth state and LiveKit metadata channel for `codex_oauth` JSON.
fn terminal_and_codex_oauth_for_livekit(
    view_factory: Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync>,
    mouse: bool,
) -> (
    tddy_service::TerminalServiceVirtualTui,
    tddy_service::CodexOAuthSession,
    tokio::sync::watch::Sender<String>,
    tokio::sync::watch::Receiver<String>,
) {
    let oauth_session: tddy_service::CodexOAuthSession = Arc::new(std::sync::Mutex::new(
        tddy_service::CodexOAuthSessionState::default(),
    ));
    let (metadata_tx, metadata_rx) = tokio::sync::watch::channel(String::new());
    let scan_buf = Arc::new(std::sync::Mutex::new(String::new()));
    let session_cl = oauth_session.clone();
    let meta_cl = metadata_tx.clone();
    let buf_cl = scan_buf.clone();
    let hook = Arc::new(move |chunk: &[u8]| {
        let mut b = match buf_cl.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        tddy_service::codex_oauth_scan::append_terminal_scan_buffer(&mut b, chunk, 65_536);
        let Some(detected) = tddy_service::codex_oauth_scan::scan_codex_oauth_from_buffer(&b)
        else {
            return;
        };
        let mut g = match session_cl.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let update = match &g.pending {
            None => true,
            Some(p) => p.detected.authorize_url != detected.authorize_url,
        };
        if !update {
            return;
        }
        g.pending = Some(tddy_service::CodexOAuthPending {
            detected: detected.clone(),
        });
        drop(g);
        let json = serde_json::json!({
            "codex_oauth": {
                "pending": true,
                "authorize_url": detected.authorize_url,
                "callback_port": detected.callback_port,
                "state": detected.state,
            }
        })
        .to_string();
        let _ = meta_cl.send(json);
    });
    let terminal_service =
        tddy_service::TerminalServiceVirtualTui::with_output_hook(view_factory, mouse, hook);
    (terminal_service, oauth_session, metadata_tx, metadata_rx)
}

/// Verify tddy-tools binary is available. Required for claude, cursor, and codex agents.
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
    tddy_core::output::set_tddy_data_dir_override(args.tddy_data_dir.clone());

    if let Err(e) = merge_session_coder_config_for_resume(&mut args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    tddy_core::output::set_tddy_data_dir_override(args.tddy_data_dir.clone());

    if let Err(e) = sync_session_dir_from_args(&mut args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = align_session_id_with_explicit_session_dir(&mut args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = apply_recipe_from_changeset_if_needed(&mut args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    clear_goal_when_not_in_recipe_goal_ids(&mut args);

    if let Err(e) = apply_agent_from_changeset_if_needed(&mut args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    // Validate args before any stderr redirect (daemon redirects stderr to a file).
    if let Err(e) = validate_web_args(&args)
        .and_then(|_| validate_livekit_args(&args))
        .and_then(|_| validate_recipe_cli(&args))
    {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    let log_config = effective_log_config(&args);
    let has_file_output = tddy_core::config_has_file_output(&log_config);
    tddy_core::init_tddy_logger(log_config);
    if let Some(session_dir) = session_artifact_dir_for_args(&args) {
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
                if let Some(dir) = session_artifact_dir_for_args(&args) {
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
    /// `{TDDY_SESSIONS_DIR}/sessions/<session_id>/` — set in [`sync_session_dir_from_args`] unless `--session-dir` overrides.
    pub session_dir: Option<PathBuf>,
    /// Tddy data root (`{this}/sessions/<session_id>/`) from `--tddy-data-dir`, `-c` / session `coder-config.yaml`. Applied in [`run_main`] before path resolution.
    pub tddy_data_dir: Option<PathBuf>,
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
    /// Session ID set at program start; drives the session directory and exit output.
    pub session_id: Option<String>,
    /// Resume from existing session (session ID). Aligns `session_id` with the resumed session.
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
    /// Path to the Codex CLI. When set, overrides `TDDY_CODEX_CLI` and the default `codex` on `PATH`.
    pub codex_cli_path: Option<PathBuf>,
    /// Path to the Codex ACP stdio agent (`codex-acp`). When set, overrides `TDDY_CODEX_ACP_CLI` and sibling/`PATH` discovery.
    pub codex_acp_cli_path: Option<PathBuf>,
    /// Run `codex login` browser OAuth (not `--device-auth`), capture authorize URL, wait for completion. Requires session directory.
    pub codex_oauth_login: bool,
    /// Workflow recipe name (`tdd`, `tdd-small`, `bugfix`, `free-prompting`, `grill-me`, `review`, `merge-pr`). `None` means default `free-prompting` or recipe from changeset on resume.
    pub recipe: Option<String>,
}

/// CLI args for tddy-coder binary: agent is claude or cursor.
#[derive(Parser, Debug, Clone)]
#[command(name = "tddy-coder")]
#[command(about = "TDD-driven coder for PRD-based development workflow")]
pub struct CoderArgs {
    /// Path to YAML config file (e.g. -c config.yaml). CLI args override config values.
    #[arg(short = 'c', long = "config")]
    pub config: Option<PathBuf>,

    /// Goal to execute: plan / reproduce (recipe start), acceptance-tests, red, green, … Omit to run full workflow.
    #[arg(long, value_parser = ["plan", "reproduce", "acceptance-tests", "red", "green", "post-green-review", "demo", "evaluate", "validate", "refactor", "update-docs"])]
    pub goal: Option<String>,

    /// Session directory for plan artifacts (default: `{TDDY_SESSIONS_DIR}/sessions/<session_id>/`). Optional override (e.g. tests).
    #[arg(long = "session-dir")]
    pub session_dir: Option<PathBuf>,

    /// Tddy data directory root (default: `$HOME/.tddy` unless `TDDY_SESSIONS_DIR` is set). Also `tddy_data_dir` in YAML.
    #[arg(long = "tddy-data-dir", value_name = "DIR")]
    pub tddy_data_dir: Option<PathBuf>,

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

    /// Agent backend: claude, claude-acp, cursor, codex, codex-acp, or stub. Omit to choose interactively at startup.
    #[arg(
        long,
        value_parser = ["claude", "claude-acp", "cursor", "codex", "codex-acp", "stub"]
    )]
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

    /// Workflow recipe: `free-prompting` (default when omitted), or `tdd`, `tdd-small`, `bugfix`, `grill-me`, `review`, `merge-pr`. Must match [`WorkflowRecipe::name`].
    #[arg(long, value_parser = ["tdd", "tdd-small", "bugfix", "free-prompting", "grill-me", "review", "merge-pr"])]
    pub recipe: Option<String>,

    /// Path to the Cursor `agent` CLI (defaults to `agent` on `PATH`, or `TDDY_CURSOR_AGENT` if set).
    #[arg(long, value_name = "PATH")]
    pub cursor_agent_path: Option<PathBuf>,

    /// Path to the Codex CLI (defaults to `codex` on `PATH`, or `TDDY_CODEX_CLI` if set).
    #[arg(long, value_name = "PATH", env = "TDDY_CODEX_CLI")]
    pub codex_cli_path: Option<PathBuf>,

    /// Path to the Codex ACP agent binary (stdio JSON-RPC). Defaults to `codex-acp` next to the
    /// resolved `codex` binary when present, else `codex-acp` on `PATH`, or `TDDY_CODEX_ACP_CLI`.
    #[arg(long, value_name = "PATH", env = "TDDY_CODEX_ACP_CLI")]
    pub codex_acp_cli_path: Option<PathBuf>,

    /// Run OpenAI Codex `login` with browser OAuth (default flow, not `--device-auth`). Writes the
    /// authorize URL to `{session_dir}/codex_oauth_authorize.url` (same as Codex `exec` + `BROWSER`
    /// hook) and waits until login finishes. Requires `--session-dir` or `--session-id`.
    #[arg(long)]
    pub codex_oauth_login: bool,
}

/// CLI args for tddy-demo binary: agent is stub only.
#[derive(Parser, Debug, Clone)]
#[command(name = "tddy-demo")]
#[command(about = "Same app as tddy-coder with StubBackend (identical TUI, CLI, workflow)")]
pub struct DemoArgs {
    /// Path to YAML config file (e.g. -c config.yaml). CLI args override config values.
    #[arg(short = 'c', long = "config")]
    pub config: Option<PathBuf>,

    /// Goal to execute: plan / reproduce, acceptance-tests, … Omit to run full workflow.
    #[arg(long, value_parser = ["plan", "reproduce", "acceptance-tests", "red", "green", "post-green-review", "demo", "evaluate", "validate", "refactor", "update-docs"])]
    pub goal: Option<String>,

    /// Session directory for plan artifacts (default: `{TDDY_SESSIONS_DIR}/sessions/<session_id>/`). Optional override (e.g. tests).
    #[arg(long = "session-dir")]
    pub session_dir: Option<PathBuf>,

    /// Tddy data directory root (default: `$HOME/.tddy` unless `TDDY_SESSIONS_DIR` is set). Also `tddy_data_dir` in YAML.
    #[arg(long = "tddy-data-dir", value_name = "DIR")]
    pub tddy_data_dir: Option<PathBuf>,

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

    /// Workflow recipe: `free-prompting` (default when omitted), or `tdd`, `tdd-small`, `bugfix`, `grill-me`, `review`, `merge-pr`.
    #[arg(long, value_parser = ["tdd", "tdd-small", "bugfix", "free-prompting", "grill-me", "review", "merge-pr"])]
    pub recipe: Option<String>,
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
            session_dir: a.session_dir,
            tddy_data_dir: a.tddy_data_dir,
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
            codex_cli_path: a.codex_cli_path,
            codex_acp_cli_path: a.codex_acp_cli_path,
            codex_oauth_login: a.codex_oauth_login,
            recipe: a.recipe,
        }
    }
}

impl From<DemoArgs> for Args {
    fn from(a: DemoArgs) -> Args {
        Args {
            goal: a.goal,
            session_dir: a.session_dir,
            tddy_data_dir: a.tddy_data_dir,
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
            codex_cli_path: None,
            codex_acp_cli_path: None,
            codex_oauth_login: false,
            recipe: a.recipe,
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
    // `--github-stub-codes` only makes sense with the stub provider; treat non-empty codes as stub
    // mode so test harnesses still get AuthService if the boolean flag is omitted or dropped.
    let stub_mode = args.github_stub
        || args
            .github_stub_codes
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty());
    if stub_mode {
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
        common_room: None,
        daemon_mode: None,
        allowed_agents: vec![],
    }
}

/// Run OpenAI Codex `login` with browser OAuth (default flow, not `--device-auth`).
fn run_codex_oauth_login(args: &Args) -> anyhow::Result<()> {
    let session_dir = session_artifact_dir_for_args(args).context(
        "--codex-oauth-login requires --session-dir or --session-id so the artifact directory is known",
    )?;
    let codex_bin = resolve_codex_binary(args.codex_cli_path.as_deref());
    let backend = CodexBackend::with_path(codex_bin);
    let url_file = session_dir.join(tddy_core::CODEX_OAUTH_AUTHORIZE_URL_FILENAME);
    eprintln!("Codex browser OAuth login (OpenAI). This is not `--device-auth`.");
    eprintln!(
        "Authorize URL file (web UI / LiveKit poller): {}",
        url_file.display()
    );
    eprintln!(
        "Complete sign-in in a browser on this host; Codex listens on localhost for the callback until finished or interrupted."
    );
    let mut child = backend
        .spawn_oauth_login(&session_dir)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let status = child.wait().context("wait on codex login")?;
    if !status.success() {
        anyhow::bail!("codex login exited with status {:?}", status.code());
    }
    eprintln!("codex login completed successfully.");
    Ok(())
}

/// Main entry point. Run the workflow with the given args.
pub fn run_with_args(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    validate_web_args(args)?;
    validate_livekit_args(args)?;
    validate_recipe_cli(args)?;
    if args.codex_oauth_login {
        return run_codex_oauth_login(args);
    }
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
        args.codex_cli_path.as_deref(),
        args.codex_acp_cli_path.as_deref(),
        None,
        None,
    );

    if args.goal.as_deref() == Some("acceptance-tests") {
        let session_dir = args.session_dir.as_ref().context("session directory")?;
        let conv = resolve_log_defaults(args, session_dir);
        let ctx = build_goal_context(args, Some(session_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "acceptance-tests", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("green") {
        let session_dir = args.session_dir.as_ref().context("session directory")?;
        let conv = resolve_log_defaults(args, session_dir);
        let ctx = build_goal_context(args, Some(session_dir), &conv, &resolved_agent, |c| {
            c.insert("run_optional_step_x".to_string(), serde_json::json!(false));
        });
        return run_goal_plain(args, backend, "green", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("evaluate") {
        let session_dir = args.session_dir.as_ref().context("session directory")?;
        let conv = resolve_log_defaults(args, session_dir);
        let ctx = build_goal_context(args, Some(session_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "evaluate", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("demo") {
        let session_dir = args.session_dir.as_ref().context("session directory")?;
        let conv = resolve_log_defaults(args, session_dir);
        let ctx = build_goal_context(args, Some(session_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "demo", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("red") {
        let session_dir = args.session_dir.as_ref().context("session directory")?;
        let conv = resolve_log_defaults(args, session_dir);
        let ctx = build_goal_context(args, Some(session_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "red", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("validate") {
        let session_dir = args.session_dir.as_ref().context("session directory")?;
        let conv = resolve_log_defaults(args, session_dir);
        let ctx = build_goal_context(args, Some(session_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "validate", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("refactor") {
        let session_dir = args.session_dir.as_ref().context("session directory")?;
        let conv = resolve_log_defaults(args, session_dir);
        let ctx = build_goal_context(args, Some(session_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "refactor", ctx, true, &shutdown);
    }

    if args.goal.as_deref() == Some("update-docs") {
        let session_dir = args.session_dir.as_ref().context("session directory")?;
        let conv = resolve_log_defaults(args, session_dir);
        let ctx = build_goal_context(args, Some(session_dir), &conv, &resolved_agent, |_| {});
        return run_goal_plain(args, backend, "update-docs", ctx, true, &shutdown);
    }

    let recipe = recipe_arc_for_args(args)?;
    let start_goal_id = recipe.start_goal();
    let start_g = start_goal_id.as_str();
    let goal_arg = args.goal.as_deref();
    let tdd_explicit_plan = recipe.name() == "tdd" && goal_arg == Some("plan");
    if goal_arg != Some(start_g) && !tdd_explicit_plan {
        anyhow::bail!(
            "unsupported goal: {} (expected `{}` for recipe `{}`{})",
            goal_arg.unwrap_or("(none)"),
            start_g,
            recipe.name(),
            if recipe.name() == "tdd" {
                ", or `plan` to skip the interview step"
            } else {
                ""
            }
        );
    }
    let goal_to_run = if tdd_explicit_plan { "plan" } else { start_g };

    let input = read_feature_input(args).context("read feature description")?;
    let input = input.trim().to_string();
    if input.is_empty() {
        anyhow::bail!("empty feature description");
    }

    let base = tddy_core::output::tddy_data_dir_path().map_err(|e| anyhow::anyhow!("{}", e))?;
    let session_dir = if let Some(ref sid) = args.session_id {
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
        recipe: Some(
            args.recipe
                .as_deref()
                .unwrap_or_else(|| crate::default_unspecified_workflow_recipe_cli_name())
                .to_string(),
        ),
        ..tddy_core::changeset::Changeset::default()
    };
    tddy_core::changeset::write_changeset(&session_dir, &init_cs)
        .map_err(|e| anyhow::anyhow!("write changeset: {}", e))?;
    tddy_core::write_initial_tool_session_metadata(
        &session_dir,
        tddy_core::InitialToolSessionMetadataOpts {
            project_id: args.project_id.clone().unwrap_or_default(),
            repo_path: Some(output_dir_for_ctx.display().to_string()),
            pid: Some(std::process::id()),
            tool: Some("tddy-coder".to_string()),
            livekit_room: None,
        },
    )
    .map_err(|e| anyhow::anyhow!("write session metadata: {}", e))?;

    let conv = resolve_log_defaults(args, &session_dir);
    let ctx = build_goal_context(args, None, &conv, &resolved_agent, |c| {
        c.insert("feature_input".to_string(), serde_json::json!(input));
        c.insert(
            "output_dir".to_string(),
            serde_json::to_value(output_dir_for_ctx).unwrap(),
        );
        c.insert(
            "session_dir".to_string(),
            serde_json::to_value(session_dir.clone()).unwrap(),
        );
    });
    run_goal_plain(args, backend, goal_to_run, ctx, true, &shutdown)
}

fn on_progress(_event: &ProgressEvent) {
    // Plain mode: progress is not displayed (no stdout/stderr per AGENTS.md)
}

/// Resolves paths for the LiveKit Virtual TUI branch inside [`run_daemon`].
///
/// `tddy_data_dir` is the same root as [`tddy_core::output::tddy_data_dir_path`]
/// (`$HOME/.tddy` or `TDDY_SESSIONS_DIR`): session artifacts live under
/// `{tddy_data_dir}/sessions/<id>/`.
///
/// Returns `(agent_working_dir, session_artifact_dir, session_dir_for_presenter)`:
/// - **agent_working_dir** — repository root for the coding agent (`InvokeRequest::working_dir`).
/// - **session_artifact_dir** — directory for session files (`PRD.md`, `changeset.yaml`, metadata,
///   logs).
/// - **session_dir_for_presenter** — `Some(artifact dir)` for both new and resumed sessions so the
///   workflow reuses the same tree as `.session.yaml` under `{tddy_data_dir}/sessions/<id>/`.
fn livekit_daemon_workflow_paths(
    tddy_data_dir: &Path,
    resume_from: Option<&str>,
    session_id: Option<&str>,
) -> (PathBuf, PathBuf, Option<PathBuf>) {
    let session_artifact_dir = resume_from
        .or(session_id)
        .map(|id| tddy_data_dir.join(SESSIONS_SUBDIR).join(id))
        .unwrap_or_else(|| {
            tddy_data_dir
                .join(SESSIONS_SUBDIR)
                .join("tddy-daemon-session")
        });

    let agent_working_dir = if resume_from.is_some() {
        read_session_metadata(&session_artifact_dir)
            .ok()
            .and_then(|m| m.repo_path)
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    let session_dir_for_presenter = Some(session_artifact_dir.clone());
    (
        agent_working_dir,
        session_artifact_dir,
        session_dir_for_presenter,
    )
}

/// Run as headless gRPC daemon. Serves GetSession and ListSessions; blocks until shutdown.
/// When LiveKit args are present, also joins the room as a participant serving RPC over the data channel.
fn run_daemon(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    // Logger is already initialized in `run_main` with `effective_log_config(args)`.
    // Do not call `init_tddy_logger` again: `log::set_logger` only succeeds once; a second
    // init would skip `set_max_level` and can leave FILE_OUTPUTS / routing inconsistent.

    let tddy_data_dir =
        tddy_core::output::tddy_data_dir_path().map_err(|e| anyhow::anyhow!("{}", e))?;
    let sessions_root = tddy_data_dir.join(tddy_core::output::SESSIONS_SUBDIR);
    std::fs::create_dir_all(&sessions_root).context("create sessions base dir")?;

    let port = args.grpc.unwrap_or(50051);
    let agent_str = args.agent.as_deref().unwrap_or("claude");
    if args.agent.is_none() {
        verify_tddy_tools_available(agent_str)?;
    }
    let backend = create_backend(
        agent_str,
        args.cursor_agent_path.as_deref(),
        args.codex_cli_path.as_deref(),
        args.codex_acp_cli_path.as_deref(),
        None,
        None,
    );
    let has_token = args.livekit_token.is_some();
    let has_key_secret = args.livekit_api_key.is_some() && args.livekit_api_secret.is_some();
    let livekit_enabled = args.livekit_url.is_some()
        && (has_token || has_key_secret)
        && args.livekit_room.is_some()
        && args.livekit_identity.is_some();

    let service = tddy_service::DaemonService::new(tddy_data_dir.clone(), backend.clone());
    #[allow(clippy::type_complexity)]
    let (view_factory, presenter_observer, presenter_intent_tx): (
        Option<Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync>>,
        Option<tddy_service::PresenterObserverService>,
        Option<std::sync::mpsc::Sender<tddy_core::UserIntent>>,
    ) = if livekit_enabled {
        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        let presenter_observer = tddy_service::PresenterObserverService::new(event_tx.clone());
        let (intent_tx, intent_rx) = std::sync::mpsc::channel();
        let presenter_intent_tx = intent_tx.clone();
        let mut presenter = Presenter::new(
            agent_str,
            args.model
                .as_deref()
                .unwrap_or_else(|| default_model_for_agent(agent_str)),
            recipe_arc_for_args(args)?,
        )
        .with_broadcast(event_tx)
        .with_intent_sender(intent_tx)
        .with_recipe_resolver(Arc::new(|name: &str| {
            crate::resolve_workflow_recipe_from_cli_name(name.trim())
        }));
        let (agent_working_dir, session_artifact_dir, session_dir) = livekit_daemon_workflow_paths(
            &tddy_data_dir,
            args.resume_from.as_deref(),
            args.session_id.as_deref(),
        );
        let _ = std::fs::create_dir_all(&session_artifact_dir);
        let logs = session_artifact_dir.join("logs");
        let _ = std::fs::create_dir_all(&logs);
        tddy_core::toolcall::set_toolcall_log_dir(&logs);

        let (toolcall_socket_path, tool_call_rx) =
            match tddy_core::toolcall::start_toolcall_listener() {
                Ok((path, rx)) => (Some(path), Some(rx)),
                Err(_) => (None, None),
            };

        tddy_core::write_initial_tool_session_metadata(
            &session_artifact_dir,
            tddy_core::InitialToolSessionMetadataOpts {
                project_id: args.project_id.clone().unwrap_or_default(),
                repo_path: std::env::current_dir()
                    .ok()
                    .map(|p| p.display().to_string()),
                pid: Some(std::process::id()),
                tool: Some("tddy-coder".to_string()),
                livekit_room: args.livekit_room.clone(),
            },
        )
        .map_err(|e| anyhow::anyhow!("write session metadata: {}", e))?;
        // New daemon sessions must not use a placeholder prompt: stdin is /dev/null from the
        // parent spawner, so the workflow must block on `answer_rx` until the user submits
        // feature text via Virtual TUI / LiveKit (SubmitFeatureInput) or Telegram `/submit-feature`
        // (PresenterIntent gRPC). A placeholder skips that and jumps straight into plan / first clarification.
        //
        // Load `initial_prompt` from `changeset.yaml` in this session dir (e.g. written by Telegram
        // before spawn) so it matches the same path as `tddy_data_dir_path()` / `TDDY_SESSIONS_DIR`.
        let initial_prompt = read_changeset(&session_artifact_dir)
            .ok()
            .and_then(|cs| cs.initial_prompt)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        presenter.start_workflow(
            backend,
            agent_working_dir,
            session_dir,
            initial_prompt,
            None,
            None,
            false,
            args.session_id.clone(),
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
        std::thread::spawn(move || loop {
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
        });
        (
            Some(factory),
            Some(presenter_observer),
            Some(presenter_intent_tx),
        )
    } else {
        (None, None, None)
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
        if let (Some(observer), Some(intent_tx_grpc)) = (presenter_observer, presenter_intent_tx) {
            grpc_router = grpc_router.add_service(
                tddy_service::gen::presenter_observer_server::PresenterObserverServer::new(
                    observer,
                ),
            );
            let intent_svc = tddy_service::PresenterIntentService::new(intent_tx_grpc);
            grpc_router = grpc_router.add_service(
                tddy_service::gen::presenter_intent_server::PresenterIntentServer::new(intent_svc),
            );
        }
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
            let (_, session_artifact_dir, _) = livekit_daemon_workflow_paths(
                &tddy_data_dir,
                args.resume_from.as_deref(),
                args.session_id.as_deref(),
            );
            let codex_oauth_watch =
                Some(session_artifact_dir.join(tddy_core::CODEX_OAUTH_AUTHORIZE_URL_FILENAME));
            let shutdown_clone = shutdown.clone();
            let factory = view_factory
                .clone()
                .expect("factory set when livekit_enabled");
            let (terminal_service, oauth_session, metadata_tx, metadata_rx) =
                terminal_and_codex_oauth_for_livekit(factory, args.mouse);
            let codex_oauth_impl = tddy_service::CodexOAuthServiceImpl::with_metadata_watch(
                oauth_session,
                metadata_tx,
            );
            let livekit_multi = tddy_rpc::MultiRpcService::new(vec![
                tddy_rpc::ServiceEntry {
                    name: "terminal.TerminalService",
                    service: std::sync::Arc::new(tddy_service::TerminalServiceServer::new(
                        terminal_service,
                    )) as std::sync::Arc<dyn tddy_rpc::RpcService>,
                },
                tddy_rpc::ServiceEntry {
                    name: tddy_service::CodexOAuthServiceServer::<
                        tddy_service::CodexOAuthServiceImpl,
                    >::NAME,
                    service: std::sync::Arc::new(tddy_service::CodexOAuthServiceServer::new(
                        codex_oauth_impl,
                    )) as std::sync::Arc<dyn tddy_rpc::RpcService>,
                },
            ]);
            if has_key_secret {
                let token_generator = tddy_livekit::TokenGenerator::new(
                    args.livekit_api_key.as_ref().unwrap().clone(),
                    args.livekit_api_secret.as_ref().unwrap().clone(),
                    args.livekit_room.as_ref().unwrap().clone(),
                    args.livekit_identity.as_ref().unwrap().clone(),
                    std::time::Duration::from_secs(120),
                );
                tokio::spawn(async move {
                    tddy_livekit::LiveKitParticipant::run_with_reconnect_metadata(
                        &url,
                        &token_generator,
                        livekit_multi,
                        tddy_livekit::RoomOptions::default(),
                        shutdown_clone,
                        Some(metadata_rx),
                        codex_oauth_watch,
                    )
                    .await
                });
            } else {
                let token = args.livekit_token.as_ref().unwrap().clone();
                tokio::spawn(async move {
                    let participant = match tddy_livekit::LiveKitParticipant::connect(
                        &url,
                        &token,
                        livekit_multi,
                        tddy_livekit::RoomOptions::default(),
                        codex_oauth_watch,
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
                    let local = participant.room().local_participant().clone();
                    let meta_task =
                        tddy_livekit::spawn_local_participant_metadata_watcher(metadata_rx, local);
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
                    meta_task.abort();
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
fn print_session_info_on_exit(session_dir: &Path) {
    let mut err = io::stderr().lock();
    let _ = write_session_hint_from_dir(&mut err, session_dir);
    let _ = err.flush();
}

/// Print session id and session dir path (uses startup session_id when only the id is known).
fn print_session_id_on_exit(session_id: &str, session_dir: &Path) {
    let mut err = io::stderr().lock();
    let _ = write_session_hint_stderr(&mut err, session_id, session_dir);
    let _ = err.flush();
}

fn write_session_hint_from_dir<W: Write>(w: &mut W, session_dir: &Path) -> io::Result<()> {
    let session_id = session_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| session_dir.display().to_string());
    write_session_hint_stderr(w, &session_id, session_dir)
}

fn write_session_hint_stderr<W: Write>(
    w: &mut W,
    session_id: &str,
    session_dir: &Path,
) -> io::Result<()> {
    writeln!(w, "Session: {}", session_id)?;
    writeln!(w, "Session dir: {}", session_dir.display())?;
    Ok(())
}

/// After the TUI exits, print workflow outcome and (when known) session directory on stderr.
fn write_post_tui_workflow_exit<Wo: Write, We: Write>(
    workflow_result: Option<Result<tddy_core::WorkflowCompletePayload, String>>,
    args: &Args,
    stdout: &mut Wo,
    stderr: &mut We,
) -> io::Result<()> {
    if let Some(result) = workflow_result {
        match &result {
            Ok(payload) => {
                writeln!(stdout, "{}", payload.summary)?;
                if let Some(ref session_dir) = payload.session_dir {
                    write_session_hint_from_dir(stderr, session_dir)?;
                }
            }
            Err(e) => {
                writeln!(stderr, "Workflow error: {}", e)?;
                if let Some(sid) = args.session_id.as_ref() {
                    let dir = session_artifact_dir_for_args(args)
                        .unwrap_or_else(|| PathBuf::from("(session dir not created)"));
                    write_session_hint_stderr(stderr, sid, &dir)?;
                }
            }
        }
    } else if let Some(sid) = args.session_id.as_ref() {
        let dir = session_artifact_dir_for_args(args)
            .unwrap_or_else(|| PathBuf::from("(session dir not created)"));
        write_session_hint_stderr(stderr, sid, &dir)?;
    }
    stdout.flush()?;
    stderr.flush()?;
    Ok(())
}

/// Compute session dir path from args (base/sessions/{session_id}/).
fn session_dir_path(args: &Args) -> Option<PathBuf> {
    let sid = args.session_id.as_deref()?;
    let base = tddy_core::output::tddy_data_dir_path().ok()?;
    Some(base.join(tddy_core::output::SESSIONS_SUBDIR).join(sid))
}

/// Session artifact root: explicit [`Args::session_dir`] if set, else [`session_dir_path`].
fn session_artifact_dir_for_args(args: &Args) -> Option<PathBuf> {
    args.session_dir.clone().or_else(|| session_dir_path(args))
}

/// When `--session-dir` is set, set [`Args::session_id`] to its final path segment when valid, so
/// logging, exit messages, and `sessions/<id>/` layout stay consistent (no extra UUID directory).
fn align_session_id_with_explicit_session_dir(args: &mut Args) -> anyhow::Result<()> {
    let Some(ref dir) = args.session_dir else {
        return Ok(());
    };
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    if tddy_core::validate_session_id_segment(name).is_ok() {
        args.session_id = Some(name.to_string());
    }
    Ok(())
}

/// Sets [`Args::session_dir`] to `{tddy_data_dir}/sessions/<session_id>/` when not overridden by `--session-dir` or config.
fn sync_session_dir_from_args(args: &mut Args) -> anyhow::Result<()> {
    if args.session_dir.is_some() {
        return Ok(());
    }
    let sid = args
        .session_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("internal: session_id missing"))?;
    let base = tddy_core::output::tddy_data_dir_path().map_err(|e| anyhow::anyhow!("{}", e))?;
    args.session_dir = Some(base.join(tddy_core::output::SESSIONS_SUBDIR).join(sid));
    Ok(())
}

/// Merges [`crate::config::SESSION_CODER_CONFIG_FILE`] from the session directory when
/// [`Args::resume_from`] is set. Uses the same YAML schema and merge rules as `-c` / [`crate::config::merge_config_into_args`].
pub fn merge_session_coder_config_for_resume(args: &mut Args) -> anyhow::Result<()> {
    let Some(ref sid) = args.resume_from else {
        return Ok(());
    };
    let base = tddy_core::output::tddy_data_dir_path().map_err(|e| anyhow::anyhow!("{}", e))?;
    let dir = base.join(tddy_core::output::SESSIONS_SUBDIR).join(sid);
    merge_session_coder_config_from_dir(args, &dir)
}

fn merge_session_coder_config_from_dir(args: &mut Args, session_dir: &Path) -> anyhow::Result<()> {
    let path = session_dir.join(crate::config::SESSION_CODER_CONFIG_FILE);
    if !path.is_file() {
        return Ok(());
    }
    let config = crate::config::load_config(&path)?;
    crate::config::merge_config_into_args(args, config);
    Ok(())
}

/// Session or global `coder-config.yaml` may set `goal: plan` from a TDD-oriented template.
/// When the selected recipe uses another start goal (e.g. bugfix → `analyze`), that stale `goal`
/// forces single-goal routing and fails before any task runs (`unsupported goal`).
///
/// Clears [`Args::goal`] when it is neither the recipe start goal nor one of [`WorkflowRecipe::goal_ids`].
fn clear_goal_when_not_in_recipe_goal_ids(args: &mut Args) {
    let recipe_name = args
        .recipe
        .as_deref()
        .unwrap_or_else(|| crate::default_unspecified_workflow_recipe_cli_name());
    let Ok(recipe) = crate::resolve_workflow_recipe_from_cli_name(recipe_name.trim()) else {
        return;
    };
    let Some(ref g) = args.goal else {
        return;
    };
    if recipe.start_goal().as_str() == g.as_str() {
        return;
    }
    let known = recipe
        .goal_ids()
        .iter()
        .any(|gid| gid.as_str() == g.as_str());
    if !known {
        log::info!(
            "clearing config goal {:?}: not a goal id for recipe {:?}",
            g,
            recipe.name()
        );
        args.goal = None;
    }
}

/// When the session dir has `changeset.yaml` with session entries, sets `agent` from
/// [`tddy_core::resolve_agent_from_changeset`] if the CLI left the default `claude`.
fn apply_agent_from_changeset_if_needed(args: &mut Args) -> anyhow::Result<()> {
    if args.agent.as_deref().is_some_and(|a| a != "claude") {
        return Ok(());
    }
    let Some(ref session_dir) = args.session_dir else {
        return Ok(());
    };
    let cs = match tddy_core::read_changeset(session_dir) {
        Ok(cs) => cs,
        Err(_) => return Ok(()),
    };
    let recipe_name = cs
        .recipe
        .as_deref()
        .unwrap_or_else(|| crate::default_unspecified_workflow_recipe_cli_name());
    let recipe = crate::resolve_workflow_recipe_from_cli_name(recipe_name.trim())
        .map_err(|e| anyhow::anyhow!(e))?;
    let start_goal_id = recipe.start_goal();
    if let Some(agent) = tddy_core::resolve_agent_from_changeset(&cs, start_goal_id.as_str()) {
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

/// Resolve Codex CLI: `--codex-cli-path` / config, then `TDDY_CODEX_CLI`, then `codex` on `PATH`.
fn resolve_codex_binary(codex_cli_path: Option<&Path>) -> PathBuf {
    codex_cli_path
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TDDY_CODEX_CLI").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(CodexBackend::DEFAULT_CLI_BINARY))
}

/// Resolve `codex-acp` stdio agent: explicit path / env, then `codex-acp` beside resolved `codex`, else `codex-acp` on `PATH`.
fn resolve_codex_acp_binary(
    codex_acp_cli_path: Option<&Path>,
    codex_cli_path: Option<&Path>,
) -> PathBuf {
    if let Some(p) = codex_acp_cli_path {
        return p.to_path_buf();
    }
    if let Some(p) = std::env::var_os("TDDY_CODEX_ACP_CLI").map(PathBuf::from) {
        return p;
    }
    let codex = resolve_codex_binary(codex_cli_path);
    if let Some(acp) = codex_acp_beside_resolved_codex(&codex) {
        return acp;
    }
    PathBuf::from(tddy_core::backend::codex_acp::DEFAULT_CODEX_ACP_BINARY)
}

/// If `codex` resolves to a concrete file, return `codex-acp` in the same directory when it exists.
fn codex_acp_beside_resolved_codex(codex: &std::path::Path) -> Option<PathBuf> {
    let codex_file = if codex.is_absolute() {
        codex.to_path_buf()
    } else if codex.components().count() == 1 {
        #[cfg(unix)]
        {
            resolve_executable_on_path(codex.as_os_str())?
        }
        #[cfg(not(unix))]
        {
            return None;
        }
    } else {
        return None;
    };
    if !codex_file.is_file() {
        return None;
    }
    let parent = codex_file.parent()?;
    let acp = parent.join(tddy_core::backend::codex_acp::DEFAULT_CODEX_ACP_BINARY);
    if acp.is_file() {
        Some(acp)
    } else {
        None
    }
}

#[cfg(unix)]
fn resolve_executable_on_path(name: &std::ffi::OsStr) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.is_file() {
            if let Ok(meta) = std::fs::metadata(&candidate) {
                if meta.permissions().mode() & 0o111 != 0 {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Create backend once at startup (plain mode, no progress events).
/// StubBackend always uses InMemoryToolExecutor (no tddy-tools): stub simulates the agent,
/// so it stores results directly. ProcessToolExecutor is for real agents (Claude/Cursor/Codex)
/// that run tddy-tools submit.
fn create_backend(
    agent: &str,
    cursor_agent_path: Option<&Path>,
    codex_cli_path: Option<&Path>,
    codex_acp_cli_path: Option<&Path>,
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
        "codex" => {
            let path = resolve_codex_binary(codex_cli_path);
            log::info!(
                "[tddy-coder] Codex backend: CLI binary `{}`",
                path.display()
            );
            AnyBackend::Codex(CodexBackend::with_path(path))
        }
        "codex-acp" => {
            let acp_bin = resolve_codex_acp_binary(codex_acp_cli_path, codex_cli_path);
            let codex_bin = resolve_codex_binary(codex_cli_path);
            log::info!(
                "[tddy-coder] Codex ACP backend: agent `{}`, codex CLI for OAuth `{}`",
                acp_bin.display(),
                codex_bin.display()
            );
            AnyBackend::CodexAcp(CodexAcpBackend::with_agent_and_codex_paths(
                acp_bin, codex_bin,
            ))
        }
        "stub" => AnyBackend::Stub(StubBackend::new()),
        _ => AnyBackend::Claude(ClaudeCodeBackend::new().with_progress(on_progress)),
    };
    SharedBackend::from_any(backend)
}

/// Resolve conversation_output and debug_output defaults to session_dir/logs/ when not set.
/// Returns the resolved conversation output path for use in context.
fn resolve_log_defaults(args: &Args, session_dir: &Path) -> Option<PathBuf> {
    tddy_core::resolve_log_defaults(args.conversation_output.clone(), None::<&Path>, session_dir)
}

/// Build context_values for a goal from args and session_dir.
fn build_goal_context(
    args: &Args,
    session_dir: Option<&PathBuf>,
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
    if let Some(ref sid) = args.session_id {
        ctx.insert("session_id".to_string(), serde_json::json!(sid.clone()));
    }
    if let Some(p) = session_dir {
        ctx.insert("session_dir".to_string(), serde_json::to_value(p).unwrap());
        let codex_tid_path = p.join(tddy_core::CODEX_THREAD_ID_FILENAME);
        if let Ok(s) = std::fs::read_to_string(&codex_tid_path) {
            let trimmed = s.trim().to_string();
            if !trimmed.is_empty() {
                ctx.insert("codex_thread_id".to_string(), serde_json::json!(trimmed));
            }
        }
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
    let storage_dir = context_values
        .get("session_dir")
        .and_then(|v| serde_json::from_value::<PathBuf>(v.clone()).ok())
        .map(|p| tddy_core::workflow::session::workflow_engine_storage_dir(&p))
        .unwrap_or_else(|| std::env::temp_dir().join("tddy-flowrunner-session"));
    std::fs::create_dir_all(&storage_dir).context("create session storage dir")?;
    let recipe = recipe_arc_for_args(args)?;
    let hooks = recipe.create_hooks(None);
    let engine = WorkflowEngine::new(recipe.clone(), backend.clone(), storage_dir, Some(hooks));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("create tokio runtime")?;

    let goal_id = GoalId::new(goal);
    let mut result = rt
        .block_on(engine.run_goal(&goal_id, context_values.clone()))
        .map_err(|e| anyhow::anyhow!("WorkflowEngine: {}", e))?;

    loop {
        match &result.status {
            ExecutionStatus::Completed | ExecutionStatus::Paused { .. } => {
                let session_opt = rt
                    .block_on(engine.get_session(&result.session_id))
                    .map_err(|e| anyhow::anyhow!("get session: {}", e))?;
                let session_dir: PathBuf = session_opt
                    .as_ref()
                    .and_then(|s| {
                        s.context
                            .get_sync("session_dir")
                            .or_else(|| s.context.get_sync("output_dir"))
                    })
                    .unwrap_or_else(|| {
                        args.session_dir
                            .clone()
                            .unwrap_or_else(|| PathBuf::from("."))
                    });
                let output: Option<String> = session_opt
                    .as_ref()
                    .and_then(|s| s.context.get_sync("output"));

                if print_output {
                    print_goal_output(goal, output.as_deref(), &session_dir, recipe.as_ref())?;
                }
                print_session_info_on_exit(&session_dir);
                return Ok(());
            }
            ExecutionStatus::ElicitationNeeded { ref event } => {
                let session_dir: PathBuf = rt
                    .block_on(engine.get_session(&result.session_id))
                    .ok()
                    .flatten()
                    .and_then(|s| {
                        s.context
                            .get_sync("session_dir")
                            .or_else(|| s.context.get_sync("output_dir"))
                    })
                    .unwrap_or_else(|| {
                        args.session_dir
                            .clone()
                            .unwrap_or_else(|| PathBuf::from("."))
                    });
                match event {
                    tddy_core::ElicitationEvent::DocumentApproval { ref content } => {
                        let mut current_prd = content.clone();
                        loop {
                            let answer = match plain::read_plan_approval_plain(&current_prd) {
                                Ok(a) => a,
                                Err(e) => {
                                    if e.downcast_ref::<std::io::Error>()
                                        .is_some_and(|io| io.kind() == io::ErrorKind::Interrupted)
                                        && shutdown.load(Ordering::Relaxed)
                                    {
                                        if let Some(sid) = args.session_id.as_ref() {
                                            let dir = session_artifact_dir_for_args(args)
                                                .unwrap_or_else(|| {
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
                            run_plan_refinement(args, &backend, &rt, &session_dir, &answer)?;
                            current_prd = recipe
                                .read_primary_session_document_utf8(&session_dir)
                                .unwrap_or_else(|| current_prd.clone());
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
                    print_goal_output(goal, output.as_deref(), &session_dir, recipe.as_ref())?;
                }
                print_session_info_on_exit(&session_dir);
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

fn print_goal_output(
    goal: &str,
    output: Option<&str>,
    session_dir: &Path,
    recipe: &dyn WorkflowRecipe,
) -> anyhow::Result<()> {
    recipe
        .plain_goal_cli_output(&GoalId::new(goal), output, session_dir)
        .map_err(|e| anyhow::anyhow!(e))
}

fn run_full_workflow_tui(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    std::env::set_var("TDDY_QUIET", "1");
    log::set_max_level(log::LevelFilter::Debug);

    if let Some(session_dir) = session_artifact_dir_for_args(args) {
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
            Presenter::new(a, m, recipe_arc_for_args(args)?)
        }
        None => {
            let m = args
                .model
                .as_deref()
                .unwrap_or_else(|| default_model_for_agent("claude"));
            Presenter::new("claude", m, recipe_arc_for_args(args)?)
        }
    }
    .with_broadcast(event_tx.clone())
    .with_intent_sender(intent_tx.clone())
    .with_recipe_resolver(Arc::new(|name: &str| {
        crate::resolve_workflow_recipe_from_cli_name(name.trim())
    }));
    let presenter = Arc::new(Mutex::new(presenter));

    if args.agent.is_none() {
        let q = backend_selection_question();
        let idx = preselected_index_for_agent("claude");
        let socket_path_for_factory = socket_path.clone();
        let cursor_path_for_factory = args.cursor_agent_path.clone();
        let codex_path_for_factory = args.codex_cli_path.clone();
        let codex_acp_path_for_factory = args.codex_acp_cli_path.clone();
        let mut p = presenter.lock().unwrap();
        p.configure_deferred_workflow_start(
            Box::new(move |agent: &str| {
                verify_tddy_tools_available(agent).map_err(|e| e.to_string())?;
                Ok(create_backend(
                    agent,
                    cursor_path_for_factory.as_deref(),
                    codex_path_for_factory.as_deref(),
                    codex_acp_path_for_factory.as_deref(),
                    socket_path_for_factory.as_deref(),
                    None,
                ))
            }),
            PendingWorkflowStart {
                output_dir: PathBuf::from("."),
                session_dir: args.session_dir.clone(),
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
            args.codex_cli_path.as_deref(),
            args.codex_acp_cli_path.as_deref(),
            socket_path.as_deref(),
            None,
        );
        presenter.lock().unwrap().start_workflow(
            backend,
            PathBuf::from("."),
            args.session_dir.clone(),
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
        let (terminal_service, oauth_session, metadata_tx, metadata_rx) =
            terminal_and_codex_oauth_for_livekit(view_factory.clone(), args.mouse);
        let codex_oauth_impl =
            tddy_service::CodexOAuthServiceImpl::with_metadata_watch(oauth_session, metadata_tx);
        let url = args.livekit_url.clone().unwrap();
        let codex_oauth_watch = args
            .session_dir
            .clone()
            .map(|d| d.join(tddy_core::CODEX_OAUTH_AUTHORIZE_URL_FILENAME));
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
            let codex_server = tddy_service::CodexOAuthServiceServer::new(codex_oauth_impl);
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
                tddy_rpc::ServiceEntry {
                    name: tddy_service::CodexOAuthServiceServer::<
                        tddy_service::CodexOAuthServiceImpl,
                    >::NAME,
                    service: std::sync::Arc::new(codex_server)
                        as std::sync::Arc<dyn tddy_rpc::RpcService>,
                },
            ]);
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(async {
                    tddy_livekit::LiveKitParticipant::run_with_reconnect_metadata(
                        &url,
                        token_generator.as_ref(),
                        multi_service,
                        tddy_livekit::RoomOptions::default(),
                        shutdown,
                        Some(metadata_rx),
                        codex_oauth_watch,
                    )
                    .await
                });
            });
        } else {
            let token = args.livekit_token.clone().unwrap();
            let livekit_multi = tddy_rpc::MultiRpcService::new(vec![
                tddy_rpc::ServiceEntry {
                    name: "terminal.TerminalService",
                    service: std::sync::Arc::new(tddy_service::TerminalServiceServer::new(
                        terminal_service,
                    )) as std::sync::Arc<dyn tddy_rpc::RpcService>,
                },
                tddy_rpc::ServiceEntry {
                    name: tddy_service::CodexOAuthServiceServer::<
                        tddy_service::CodexOAuthServiceImpl,
                    >::NAME,
                    service: std::sync::Arc::new(tddy_service::CodexOAuthServiceServer::new(
                        codex_oauth_impl,
                    )) as std::sync::Arc<dyn tddy_rpc::RpcService>,
                },
            ]);
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                let watch = codex_oauth_watch;
                rt.block_on(async {
                    match tddy_livekit::LiveKitParticipant::connect(
                        &url,
                        &token,
                        livekit_multi,
                        tddy_livekit::RoomOptions::default(),
                        watch,
                    )
                    .await
                    {
                        Ok(participant) => {
                            log::info!("READY");
                            let local = participant.room().local_participant().clone();
                            let meta_task = tddy_livekit::spawn_local_participant_metadata_watcher(
                                metadata_rx,
                                local,
                            );
                            participant.run().await;
                            meta_task.abort();
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
    let presenter_handle = std::thread::spawn(move || loop {
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

    let workflow_result = presenter.lock().unwrap().take_workflow_result();
    write_post_tui_workflow_exit(
        workflow_result,
        args,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )?;

    Ok(())
}

fn run_full_workflow_plain(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    let agent_str = resolve_agent_for_full_workflow_plain(args)?;
    let backend = create_backend(
        &agent_str,
        args.cursor_agent_path.as_deref(),
        args.codex_cli_path.as_deref(),
        args.codex_acp_cli_path.as_deref(),
        None,
        None,
    );

    let recipe = recipe_arc_for_args(args)?;
    let mut session_dir = args.session_dir.clone().context("session directory")?;
    if recipe.uses_primary_session_document()
        && recipe
            .read_primary_session_document_utf8(&session_dir)
            .is_none()
    {
        std::fs::create_dir_all(&session_dir).context("create session dir")?;
        run_plan_bootstrap_in_session_dir(
            args,
            session_dir.as_path(),
            backend.clone(),
            &agent_str,
            &shutdown,
        )?;
    }

    // When the session is at the recipe's initial state (legacy TDD `Init` included) and no primary
    // session document (or no start-goal session), run start goal to complete it.
    let cs_pre = read_changeset(&session_dir).ok();
    let start_goal_id = recipe.start_goal();
    let start_tag_str = start_goal_id.as_str();
    let plan_needs_completion = cs_pre.as_ref().is_some_and(|c| {
        let st = c.state.current.as_str();
        let at_initial =
            st == recipe.initial_state().as_str() || (recipe.name() == "tdd" && st == "Init");
        recipe.uses_primary_session_document()
            && at_initial
            && (recipe
                .read_primary_session_document_utf8(&session_dir)
                .is_none()
                || get_session_for_tag(c, start_tag_str).is_none())
    });
    if plan_needs_completion {
        let input = cs_pre
            .as_ref()
            .and_then(|c| c.initial_prompt.as_deref())
            .unwrap_or("feature")
            .trim()
            .to_string();
        if !input.is_empty() {
            session_dir = run_plan_to_complete(
                args,
                backend.clone(),
                &input,
                &session_dir,
                &agent_str,
                &shutdown,
            )?;
        }
    }

    let run_optional_step_x = session_dir.join("demo-plan.md").exists()
        && plain::read_demo_choice_plain().context("read demo choice")?;

    let cs = read_changeset(&session_dir).ok();
    let start_goal = match cs.as_ref() {
        Some(c) => tddy_core::start_goal_for_session_continue(recipe.as_ref(), c),
        None => recipe.start_goal(),
    };
    let start_is_full = start_goal == recipe.start_goal();

    let storage_dir = tddy_core::workflow::session::workflow_engine_storage_dir(&session_dir);
    std::fs::create_dir_all(&storage_dir).context("create session storage dir")?;
    let hooks = recipe.create_hooks(None);
    let backend_for_refine = backend.clone();
    let engine = WorkflowEngine::new(recipe.clone(), backend, storage_dir, Some(hooks));

    let feature_input = cs_pre
        .as_ref()
        .and_then(|c| c.initial_prompt.as_deref())
        .or(args.prompt.as_deref())
        .unwrap_or("feature")
        .trim()
        .to_string();
    let conv = resolve_log_defaults(args, &session_dir);
    // output_dir comes from build_goal_context (repo_path in changeset); do not overwrite with session_dir.parent()
    // — session_dir under ~/.tddy/sessions/ would make parent wrong for worktree creation.
    let context_values = build_goal_context(args, Some(&session_dir), &conv, &agent_str, |c| {
        c.insert(
            "feature_input".to_string(),
            serde_json::json!(feature_input),
        );
        c.insert(
            "run_optional_step_x".to_string(),
            serde_json::json!(run_optional_step_x),
        );
    });

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("create tokio runtime")?;

    let mut result = if start_is_full {
        rt.block_on(engine.run_full_workflow(context_values))
    } else {
        rt.block_on(engine.run_workflow_from(&start_goal, context_values))
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
                let session_dir_final: PathBuf = session_opt
                    .as_ref()
                    .and_then(|s| {
                        s.context
                            .get_sync("session_dir")
                            .or_else(|| s.context.get_sync("output_dir"))
                    })
                    .unwrap_or(session_dir.clone());
                if let Some(ref out) = output {
                    if let Ok(refactor_out) = parse_refactor_response(out) {
                        if let Ok(eval_content) =
                            std::fs::read_to_string(session_dir_final.join("evaluation-report.md"))
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
                println!("\nSession dir: {}", session_dir_final.display());
                print_session_info_on_exit(&session_dir_final);
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
                    tddy_core::ElicitationEvent::DocumentApproval { ref content } => {
                        let mut current_prd = content.clone();
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
                                &session_dir,
                                &answer,
                            )?;
                            current_prd = recipe
                                .read_primary_session_document_utf8(&session_dir)
                                .unwrap_or_else(|| current_prd.clone());
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

/// Writes initial changeset and `.session.yaml`, then runs the recipe start goal in `session_dir`.
///
/// Used when the primary session document (e.g. `fix-plan.md`) is missing so the plan step must
/// run before the full workflow. The directory is the resolved CLI session path (including
/// explicit `--session-dir`), not a newly allocated UUID folder.
fn run_plan_bootstrap_in_session_dir(
    args: &Args,
    session_dir: &Path,
    backend: SharedBackend,
    resolved_agent_for_model: &str,
    shutdown: &AtomicBool,
) -> anyhow::Result<()> {
    let input = read_feature_input(args).context("read feature description")?;
    let input = input.trim().to_string();
    if input.is_empty() {
        anyhow::bail!("empty feature description");
    }
    let output_dir_for_ctx =
        std::env::current_dir().context("current dir for agent working_dir")?;
    let recipe = recipe_arc_for_args(args)?;
    let start_goal_id = recipe.start_goal();
    let start_g = start_goal_id.as_str();
    let init_cs = tddy_core::changeset::Changeset {
        initial_prompt: Some(input.clone()),
        repo_path: Some(output_dir_for_ctx.display().to_string()),
        recipe: Some(
            args.recipe
                .as_deref()
                .unwrap_or_else(|| crate::default_unspecified_workflow_recipe_cli_name())
                .to_string(),
        ),
        ..tddy_core::changeset::Changeset::default()
    };
    tddy_core::changeset::write_changeset(session_dir, &init_cs)
        .map_err(|e| anyhow::anyhow!("write changeset: {}", e))?;
    tddy_core::write_initial_tool_session_metadata(
        session_dir,
        tddy_core::InitialToolSessionMetadataOpts {
            project_id: args.project_id.clone().unwrap_or_default(),
            repo_path: Some(output_dir_for_ctx.display().to_string()),
            pid: Some(std::process::id()),
            tool: Some("tddy-coder".to_string()),
            livekit_room: None,
        },
    )
    .map_err(|e| anyhow::anyhow!("write session metadata: {}", e))?;

    let session_dir_buf = session_dir.to_path_buf();
    let conv = resolve_log_defaults(args, &session_dir_buf);
    let ctx = build_goal_context(
        args,
        Some(&session_dir_buf),
        &conv,
        resolved_agent_for_model,
        |c| {
            c.insert("feature_input".to_string(), serde_json::json!(input));
        },
    );
    run_goal_plain(args, backend, start_g, ctx, false, shutdown)?;
    Ok(())
}

fn run_plan_to_complete(
    args: &Args,
    backend: SharedBackend,
    input: &str,
    session_dir: &PathBuf,
    resolved_agent_for_model: &str,
    shutdown: &AtomicBool,
) -> anyhow::Result<PathBuf> {
    // output_dir from build_goal_context (repo_path in changeset); session_dir.parent() wrong when under ~/.tddy/sessions/
    let conv = resolve_log_defaults(args, session_dir);
    let ctx = build_goal_context(
        args,
        Some(session_dir),
        &conv,
        resolved_agent_for_model,
        |c| {
            c.insert("feature_input".to_string(), serde_json::json!(input));
        },
    );
    let recipe = recipe_arc_for_args(args)?;
    let start_goal_id = recipe.start_goal();
    let start_g = start_goal_id.as_str();
    run_goal_plain(args, backend, start_g, ctx, false, shutdown)?;
    Ok(session_dir.clone())
}

/// Run plan refinement: re-run the plan goal with feedback, handling clarification.
fn run_plan_refinement(
    args: &Args,
    backend: &SharedBackend,
    rt: &tokio::runtime::Runtime,
    session_dir: &Path,
    feedback: &str,
) -> anyhow::Result<()> {
    let feature_input = read_changeset(session_dir)
        .ok()
        .and_then(|c| c.initial_prompt.clone())
        .unwrap_or_else(|| "feature".to_string());
    let recipe = recipe_arc_for_args(args)?;
    let refine_goal = recipe.plan_refinement_goal();
    let refine_tag = refine_goal.as_str();
    let session_id_for_refine = read_changeset(session_dir)
        .ok()
        .and_then(|c| get_session_for_tag(&c, refine_tag));
    // output_dir from build_goal_context (repo_path in changeset); session_dir.parent() wrong when under ~/.tddy/sessions/
    let refine_storage = tddy_core::workflow::session::workflow_engine_storage_dir(session_dir);
    std::fs::create_dir_all(&refine_storage).context("create refine session dir")?;
    let refine_hooks = recipe.create_hooks(None);
    let refine_engine = WorkflowEngine::new(
        recipe.clone(),
        backend.clone(),
        refine_storage,
        Some(refine_hooks),
    );
    let session_dir_buf = session_dir.to_path_buf();
    let conv = resolve_log_defaults(args, &session_dir_buf);
    let mut refine_ctx =
        build_goal_context(args, Some(&session_dir_buf), &conv, backend.name(), |c| {
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
        .block_on(refine_engine.run_goal(&refine_goal, refine_ctx))
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
            session_dir: None,
            tddy_data_dir: None,
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
            codex_cli_path: None,
            codex_acp_cli_path: None,
            codex_oauth_login: false,
            recipe: None,
        };

        merge_session_coder_config_for_resume(&mut args).expect("merge");

        assert_eq!(args.agent.as_deref(), Some("cursor"));
        assert_eq!(
            args.cursor_agent_path.as_deref(),
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
            session_dir: None,
            tddy_data_dir: None,
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
            codex_cli_path: None,
            codex_acp_cli_path: None,
            codex_oauth_login: false,
            recipe: None,
        };

        assign_default_session_id(&mut args);

        assert_eq!(args.session_id.as_deref(), Some(sid));
    }
}

#[cfg(test)]
mod session_dir_sync_tests {
    use super::sync_session_dir_from_args;
    use super::Args;
    use serial_test::serial;

    #[test]
    #[serial]
    fn session_dir_derived_from_session_id_when_unset() {
        let tmp =
            std::env::temp_dir().join(format!("tddy-plan-dir-session-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create temp sessions base");
        std::env::set_var(tddy_core::output::TDDY_SESSIONS_DIR_ENV, &tmp);

        let sid = "019d105b-ac0f-78d3-9a89-409731145a36";
        let expected = tmp.join("sessions").join(sid);

        let mut args = Args {
            goal: None,
            session_dir: None,
            tddy_data_dir: None,
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
            codex_cli_path: None,
            codex_acp_cli_path: None,
            codex_oauth_login: false,
            recipe: None,
        };

        sync_session_dir_from_args(&mut args).expect("apply");

        assert_eq!(args.session_dir, Some(expected));

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
    use tddy_core::WorkflowState;

    /// Backend `agent` should follow the plan session recorded in `changeset.yaml` on resume.
    #[test]
    #[serial]
    fn resume_applies_agent_from_changeset_plan_session() {
        let session_dir =
            std::env::temp_dir().join(format!("tddy-changeset-agent-tests-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&session_dir);
        std::fs::create_dir_all(&session_dir).expect("plan dir");

        let mut cs = Changeset::default();
        append_session_and_update_state(
            &mut cs,
            "plan-sess".into(),
            "plan",
            WorkflowState::new("Planned"),
            "cursor",
            None,
        );
        write_changeset(&session_dir, &cs).expect("write changeset");

        let sid = "019d105b-ac0f-78d3-9a89-409731145a36";
        let mut args = Args {
            goal: None,
            session_dir: Some(session_dir.clone()),
            tddy_data_dir: None,
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
            codex_cli_path: None,
            codex_acp_cli_path: None,
            codex_oauth_login: false,
            recipe: None,
        };

        apply_agent_from_changeset_if_needed(&mut args).expect("apply");

        assert_eq!(args.agent.as_deref(), Some("cursor"));

        let _ = std::fs::remove_dir_all(&session_dir);
    }
}

#[cfg(test)]
mod livekit_daemon_path_contract_tests {
    use super::livekit_daemon_workflow_paths;
    use std::ffi::OsStr;

    #[test]
    fn resume_does_not_alias_agent_working_directory_with_session_directory() {
        let base =
            std::env::temp_dir().join(format!("tddy-livekit-path-resume-{}", std::process::id()));
        std::fs::create_dir_all(base.join("sessions")).unwrap();
        let sid = "a97addd3-c31b-442b-a6b0-a63abe99e11d";
        let (working_dir, session_artifact_dir, presenter_dir) =
            livekit_daemon_workflow_paths(&base, Some(sid), None);
        assert_eq!(
            session_artifact_dir,
            base.join("sessions").join(sid),
            "artifact dir must be {}/sessions/<id>/ (same contract as gRPC daemon)",
            base.display()
        );
        assert_ne!(working_dir, session_artifact_dir);
        assert_eq!(presenter_dir, Some(session_artifact_dir));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn new_livekit_session_does_not_use_sessions_subtree_as_agent_working_directory() {
        let base =
            std::env::temp_dir().join(format!("tddy-livekit-path-new-{}", std::process::id()));
        std::fs::create_dir_all(base.join("sessions")).unwrap();
        let (working_dir, artifact, presenter_dir) =
            livekit_daemon_workflow_paths(&base, None, None);
        assert_eq!(presenter_dir, Some(artifact.clone()));
        assert_ne!(
            working_dir.parent().and_then(|p| p.file_name()),
            Some(OsStr::new("sessions")),
            "agent working directory must be the repository root, not under .../sessions/"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}

#[cfg(test)]
mod post_tui_workflow_exit_tests {
    use super::{sync_session_dir_from_args, write_post_tui_workflow_exit, Args};
    use serial_test::serial;

    fn minimal_args(session_id: &str) -> Args {
        Args {
            goal: None,
            session_dir: None,
            tddy_data_dir: None,
            conversation_output: None,
            model: None,
            allowed_tools: None,
            log: None,
            log_level: None,
            agent: None,
            prompt: None,
            grpc: None,
            session_id: Some(session_id.to_string()),
            resume_from: None,
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
            codex_cli_path: None,
            codex_acp_cli_path: None,
            codex_oauth_login: false,
            recipe: None,
        }
    }

    /// When the TUI path finishes with `WorkflowComplete(Err(..))` (e.g. green output parse failure),
    /// stderr must still print session id and session dir so the user can open `changeset.yaml`
    /// and inspect the worktree path.
    #[test]
    #[serial]
    fn workflow_error_after_tui_includes_session_hint_on_stderr() {
        let tmp = std::env::temp_dir().join(format!("tddy-post-tui-exit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var(tddy_core::output::TDDY_SESSIONS_DIR_ENV, &tmp);

        let sid = "019d105b-ac0f-78d3-9a89-409731145a36";
        let mut args = minimal_args(sid);
        sync_session_dir_from_args(&mut args).unwrap();

        let mut out = Vec::new();
        let mut err = Vec::new();
        write_post_tui_workflow_exit(
            Some(Err(
                "output parsing failed: malformed output: Agent finished without calling tddy-tools submit for goal 'green'"
                    .into(),
            )),
            &args,
            &mut out,
            &mut err,
        )
        .unwrap();

        std::env::remove_var(tddy_core::output::TDDY_SESSIONS_DIR_ENV);
        let _ = std::fs::remove_dir_all(&tmp);

        let err_s = String::from_utf8(err).expect("utf8 stderr");
        assert!(
            err_s.contains("Workflow error:"),
            "expected workflow error line, got: {:?}",
            err_s
        );
        assert!(
            err_s.contains("Session:") && err_s.contains("Session dir:"),
            "stderr must include Session and Session dir when workflow returns Err; got: {:?}",
            err_s
        );
        assert!(
            err_s.contains(sid),
            "stderr should mention session id {}; got {:?}",
            sid,
            err_s
        );
    }
}

#[cfg(test)]
mod start_goal_for_session_continue_contract_tests {
    use std::sync::Arc;
    use tddy_core::changeset::{Changeset, StateTransition};
    use tddy_core::start_goal_for_session_continue;
    use tddy_core::workflow::ids::WorkflowState;
    use tddy_core::{GoalId, WorkflowRecipe};
    use tddy_workflow_recipes::{BugfixRecipe, TddRecipe};

    fn failed_after_green_implementing_tdd() -> Changeset {
        let mut cs = Changeset::default();
        cs.state.current = WorkflowState::new("Failed");
        cs.state.history = vec![
            StateTransition {
                state: WorkflowState::new("RedTestsReady"),
                at: "t1".into(),
            },
            StateTransition {
                state: WorkflowState::new("GreenImplementing"),
                at: "t2".into(),
            },
            StateTransition {
                state: WorkflowState::new("Failed"),
                at: "t3".into(),
            },
        ];
        cs
    }

    #[test]
    fn tdd_failed_after_green_implementing_resumes_green() {
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TddRecipe);
        let g = start_goal_for_session_continue(
            recipe.as_ref(),
            &failed_after_green_implementing_tdd(),
        );
        assert_eq!(g, GoalId::new("green"));
    }

    #[test]
    fn tdd_failed_empty_history_falls_back_to_start_goal() {
        let mut cs = Changeset::default();
        cs.state.current = WorkflowState::new("Failed");
        cs.state.history.clear();
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TddRecipe);
        let g = start_goal_for_session_continue(recipe.as_ref(), &cs);
        assert_eq!(g, GoalId::new("interview"));
    }

    /// `Planning` immediately before `Failed` (e.g. full workflow restarted plan) must not hide
    /// an earlier `GreenImplementing` when choosing the resume goal.
    #[test]
    fn tdd_failed_skips_trailing_planning_for_earlier_green_implementing() {
        let mut cs = Changeset::default();
        cs.state.current = WorkflowState::new("Failed");
        cs.state.history = vec![
            StateTransition {
                state: WorkflowState::new("RedTestsReady"),
                at: "t1".into(),
            },
            StateTransition {
                state: WorkflowState::new("GreenImplementing"),
                at: "t2".into(),
            },
            StateTransition {
                state: WorkflowState::new("Planning"),
                at: "t3".into(),
            },
            StateTransition {
                state: WorkflowState::new("Failed"),
                at: "t4".into(),
            },
        ];
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TddRecipe);
        let g = start_goal_for_session_continue(recipe.as_ref(), &cs);
        assert_eq!(g, GoalId::new("green"));
    }

    #[test]
    fn tdd_failed_skips_trailing_planning_for_earlier_planned() {
        let mut cs = Changeset::default();
        cs.state.current = WorkflowState::new("Failed");
        cs.state.history = vec![
            StateTransition {
                state: WorkflowState::new("Planned"),
                at: "t1".into(),
            },
            StateTransition {
                state: WorkflowState::new("Planning"),
                at: "t2".into(),
            },
            StateTransition {
                state: WorkflowState::new("Failed"),
                at: "t3".into(),
            },
        ];
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TddRecipe);
        let g = start_goal_for_session_continue(recipe.as_ref(), &cs);
        assert_eq!(g, GoalId::new("acceptance-tests"));
    }

    /// Manual `changeset.yaml` edits often append the right `history` tail but leave `current`
    /// stale (e.g. still `Planning`). Resume must not use `next_goal_for_state(current)` in that case.
    #[test]
    fn tdd_history_tail_failed_resumes_green_even_when_current_stale_planning() {
        let mut cs = Changeset::default();
        cs.state.current = WorkflowState::new("Planning");
        cs.state.history = vec![
            StateTransition {
                state: WorkflowState::new("RedTestsReady"),
                at: "t1".into(),
            },
            StateTransition {
                state: WorkflowState::new("GreenImplementing"),
                at: "t2".into(),
            },
            StateTransition {
                state: WorkflowState::new("Failed"),
                at: "t3".into(),
            },
        ];
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TddRecipe);
        let g = start_goal_for_session_continue(recipe.as_ref(), &cs);
        assert_eq!(g, GoalId::new("green"));
    }

    #[test]
    fn tdd_green_implementing_not_failed_uses_next_goal() {
        let mut cs = Changeset::default();
        cs.state.current = WorkflowState::new("GreenImplementing");
        cs.state.history.push(StateTransition {
            state: WorkflowState::new("GreenImplementing"),
            at: "t".into(),
        });
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TddRecipe);
        let g = start_goal_for_session_continue(recipe.as_ref(), &cs);
        assert_eq!(g, GoalId::new("green"));
    }

    #[test]
    fn bugfix_failed_after_reproducing_resumes_reproduce() {
        let mut cs = Changeset::default();
        cs.state.current = WorkflowState::new("Failed");
        cs.state.history = vec![
            StateTransition {
                state: WorkflowState::new("Reproducing"),
                at: "t1".into(),
            },
            StateTransition {
                state: WorkflowState::new("Failed"),
                at: "t2".into(),
            },
        ];
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(BugfixRecipe);
        let g = start_goal_for_session_continue(recipe.as_ref(), &cs);
        assert_eq!(g, GoalId::new("reproduce"));
    }
}
