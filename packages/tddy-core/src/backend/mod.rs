//! Coding backend abstraction for LLM-based coders.

mod acp;
mod claude;
mod codex;
pub mod codex_acp;
mod cursor;
mod mock;
mod stub;
mod tool_executor;

pub use acp::ClaudeAcpBackend;
pub use claude::{
    build_claude_args, read_claude_transcript_usage, ClaudeCodeBackend, ClaudeInvokeConfig,
    PermissionMode,
};
pub(crate) use codex::write_codex_thread_id_file;
pub use codex::{CodexBackend, CODEX_OAUTH_AUTHORIZE_URL_FILENAME, CODEX_THREAD_ID_FILENAME};
pub use codex_acp::CodexAcpBackend;
pub use cursor::CursorBackend;
pub use mock::MockBackend;
pub use stub::StubBackend;
pub use tool_executor::{InMemoryToolExecutor, ProcessToolExecutor, ToolExecutor};

/// Enum dispatch for CLI backend selection (avoids trait object overhead).
/// tddy-coder uses claude/cursor only. tddy-demo uses stub (via lib, not CLI).
#[derive(Debug)]
pub enum AnyBackend {
    Claude(ClaudeCodeBackend),
    ClaudeAcp(ClaudeAcpBackend),
    Cursor(CursorBackend),
    /// OpenAI Codex CLI (`codex exec`, `codex exec resume`, `--json`).
    Codex(CodexBackend),
    /// OpenAI Codex via `codex-acp` (ACP over stdio).
    CodexAcp(CodexAcpBackend),
    Stub(StubBackend),
}

/// Shared backend wrapper for "create once at startup" pattern.
/// Wraps `Arc<dyn CodingBackend>` so the same backend can be reused across multiple Workflows.
#[derive(Clone)]
pub struct SharedBackend(std::sync::Arc<dyn CodingBackend>);

impl std::fmt::Debug for SharedBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SharedBackend({})", self.0.name())
    }
}

#[async_trait::async_trait]
impl CodingBackend for SharedBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        self.0.invoke(request).await
    }

    fn name(&self) -> &str {
        self.0.name()
    }

    fn submit_channel(&self) -> Option<&crate::toolcall::SubmitResultChannel> {
        self.0.submit_channel()
    }

    fn action_invoke_cache_eligible(&self) -> bool {
        self.0.action_invoke_cache_eligible()
    }

    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        self.0.list_models().await
    }
}

impl SharedBackend {
    /// Create a SharedBackend from an AnyBackend (or any CodingBackend).
    pub fn from_any(backend: AnyBackend) -> Self {
        Self(std::sync::Arc::new(backend))
    }

    /// Create SharedBackend from an Arc<dyn CodingBackend> (e.g. for MockBackend in tests).
    pub fn from_arc(inner: std::sync::Arc<dyn CodingBackend>) -> Self {
        Self(inner)
    }

    /// Get the inner Arc for use with graph builders that require Arc<dyn CodingBackend>.
    pub fn as_arc(&self) -> std::sync::Arc<dyn CodingBackend> {
        self.0.clone()
    }
}

#[async_trait::async_trait]
impl CodingBackend for AnyBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        match self {
            AnyBackend::Claude(b) => b.invoke(request).await,
            AnyBackend::ClaudeAcp(b) => b.invoke(request).await,
            AnyBackend::Cursor(b) => b.invoke(request).await,
            AnyBackend::Codex(b) => b.invoke(request).await,
            AnyBackend::CodexAcp(b) => b.invoke(request).await,
            AnyBackend::Stub(b) => b.invoke(request).await,
        }
    }

    fn name(&self) -> &str {
        match self {
            AnyBackend::Claude(b) => b.name(),
            AnyBackend::ClaudeAcp(b) => b.name(),
            AnyBackend::Cursor(b) => b.name(),
            AnyBackend::Codex(b) => b.name(),
            AnyBackend::CodexAcp(b) => b.name(),
            AnyBackend::Stub(b) => b.name(),
        }
    }

    fn submit_channel(&self) -> Option<&crate::toolcall::SubmitResultChannel> {
        match self {
            AnyBackend::Claude(b) => b.submit_channel(),
            AnyBackend::ClaudeAcp(b) => b.submit_channel(),
            AnyBackend::Cursor(b) => b.submit_channel(),
            AnyBackend::Codex(b) => b.submit_channel(),
            AnyBackend::CodexAcp(b) => b.submit_channel(),
            AnyBackend::Stub(b) => b.submit_channel(),
        }
    }

    fn action_invoke_cache_eligible(&self) -> bool {
        match self {
            AnyBackend::Claude(b) => b.action_invoke_cache_eligible(),
            AnyBackend::ClaudeAcp(b) => b.action_invoke_cache_eligible(),
            AnyBackend::Cursor(b) => b.action_invoke_cache_eligible(),
            AnyBackend::Codex(b) => b.action_invoke_cache_eligible(),
            AnyBackend::CodexAcp(b) => b.action_invoke_cache_eligible(),
            AnyBackend::Stub(b) => b.action_invoke_cache_eligible(),
        }
    }

    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        match self {
            AnyBackend::Claude(b) => b.list_models().await,
            AnyBackend::ClaudeAcp(b) => b.list_models().await,
            AnyBackend::Cursor(b) => b.list_models().await,
            AnyBackend::Codex(b) => b.list_models().await,
            AnyBackend::CodexAcp(b) => b.list_models().await,
            AnyBackend::Stub(b) => b.list_models().await,
        }
    }
}

use crate::error::BackendError;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static CHILD_PID: AtomicU32 = AtomicU32::new(0);

/// Record the PID of a spawned child process so the SIGINT handler can kill it.
pub fn set_child_pid(pid: u32) {
    CHILD_PID.store(pid, Ordering::SeqCst);
}

/// Clear the child PID after the child has exited.
pub fn clear_child_pid() {
    CHILD_PID.store(0, Ordering::SeqCst);
}

/// Return the currently tracked child PID, or 0 if none.
pub fn get_child_pid() -> u32 {
    CHILD_PID.load(Ordering::SeqCst)
}

/// Kill the tracked child process. Returns true if the kill signal was delivered.
#[cfg(unix)]
pub fn kill_child_process() -> bool {
    let pid = CHILD_PID.swap(0, Ordering::SeqCst);
    if pid == 0 {
        return false;
    }
    unsafe { libc::kill(pid as i32, libc::SIGKILL) == 0 }
}

/// Format binary + args as a shell-like command for debug logging.
/// Truncates args longer than max_arg_len to keep logs readable.
pub(crate) fn format_command_for_log(
    binary: &std::path::Path,
    args: &[String],
    max_arg_len: usize,
) -> String {
    let mut parts = vec![binary.display().to_string()];
    for arg in args {
        let s = if arg.len() > max_arg_len {
            format!(
                "{}... ({} chars total)",
                &arg[..arg.floor_char_boundary(max_arg_len)],
                arg.len()
            )
        } else {
            arg.clone()
        };
        let escaped = if s.contains(' ') || s.contains('"') || s.contains('\n') {
            format!(
                "\"{}\"",
                s.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
            )
        } else {
            s
        };
        parts.push(escaped);
    }
    parts.join(" ")
}

/// Non-unix stub: clears the tracked PID but cannot actually kill the process.
#[cfg(not(unix))]
pub fn kill_child_process() -> bool {
    let pid = CHILD_PID.swap(0, Ordering::SeqCst);
    if pid == 0 {
        return false;
    }
    log::warn!(
        "[tddy-core] kill_child_process: cannot kill pid {} on non-unix platform",
        pid
    );
    false
}

pub use crate::workflow::ids::GoalId;
pub use crate::workflow::recipe::{GoalHints, PermissionHint, WorkflowRecipe};

/// Sink for routing agent output (e.g. to TUI instead of stderr).
#[derive(Clone)]
pub struct AgentOutputSink(std::sync::Arc<dyn Fn(&str) + Send + Sync>);

impl std::fmt::Debug for AgentOutputSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<agent_output_sink>")
    }
}

/// Sink for routing progress events (ToolUse, TaskStarted, TaskProgress) to TUI.
#[derive(Clone)]
pub struct ProgressSink(std::sync::Arc<dyn Fn(&crate::stream::ProgressEvent) + Send + Sync>);

impl std::fmt::Debug for ProgressSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<progress_sink>")
    }
}

impl ProgressSink {
    /// Create a sink from a closure.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&crate::stream::ProgressEvent) + Send + Sync + 'static,
    {
        Self(std::sync::Arc::new(f))
    }

    /// Invoke the sink with the given event.
    pub fn emit(&self, ev: &crate::stream::ProgressEvent) {
        (self.0)(ev);
    }
}

impl AgentOutputSink {
    /// Create a sink from a closure.
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        Self(std::sync::Arc::new(f))
    }

    /// Invoke the sink with the given text.
    pub fn emit(&self, s: &str) {
        (self.0)(s);
    }
}

/// Session mode for backend invocation: fresh session or resume existing.
#[derive(Debug, Clone)]
pub enum SessionMode {
    /// Start a new session with this ID.
    Fresh(String),
    /// Resume an existing session.
    Resume(String),
}

impl SessionMode {
    /// Session ID (same for both variants).
    pub fn session_id(&self) -> &str {
        match self {
            SessionMode::Fresh(id) | SessionMode::Resume(id) => id,
        }
    }

    /// True when resuming.
    pub fn is_resume(&self) -> bool {
        matches!(self, SessionMode::Resume(_))
    }
}

/// Environment for remote-codebase mode: the relay daemon address + session credentials.
///
/// When set on `InvokeRequest`, the Claude backend exports these as `TDDY_REMOTE_*` env vars
/// before spawning the subprocess so that the inherited `tddy-tools --mcp` routes correctly.
#[derive(Debug, Clone)]
pub struct RemoteToolEnv {
    pub daemon_url: String,
    pub session_id: String,
    pub session_token: String,
    pub daemon_instance_id: Option<String>,
    pub livekit_url: Option<String>,
    pub livekit_room: Option<String>,
    pub server_identity: Option<String>,
}

impl RemoteToolEnv {
    /// Returns all TDDY_REMOTE_* key-value pairs to be set as environment variables.
    pub fn env_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = vec![
            (
                "TDDY_REMOTE_DAEMON_URL".to_string(),
                self.daemon_url.clone(),
            ),
            (
                "TDDY_REMOTE_SESSION_ID".to_string(),
                self.session_id.clone(),
            ),
            (
                "TDDY_REMOTE_SESSION_TOKEN".to_string(),
                self.session_token.clone(),
            ),
        ];
        if let Some(v) = &self.daemon_instance_id {
            pairs.push(("TDDY_REMOTE_DAEMON_INSTANCE_ID".to_string(), v.clone()));
        }
        if let Some(v) = &self.livekit_url {
            pairs.push(("TDDY_REMOTE_LIVEKIT_URL".to_string(), v.clone()));
        }
        if let Some(v) = &self.livekit_room {
            pairs.push(("TDDY_REMOTE_LIVEKIT_ROOM".to_string(), v.clone()));
        }
        if let Some(v) = &self.server_identity {
            pairs.push(("TDDY_REMOTE_SERVER_IDENTITY".to_string(), v.clone()));
        }
        pairs
    }
}

/// Request to invoke the coding backend.
#[derive(Debug, Clone)]
pub struct InvokeRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    /// When set, backend uses this path instead of system_prompt (avoids temp file).
    pub system_prompt_path: Option<PathBuf>,
    pub goal_id: GoalId,
    /// Key for `tddy-tools submit` / progress events (may differ from graph task id, e.g. evaluate vs evaluate-changes).
    pub submit_key: GoalId,
    pub hints: GoalHints,
    /// Optional model name (e.g. "sonnet") passed to the agent.
    pub model: Option<String>,
    /// Session mode: Claude uses `--session-id` / `--resume`; Cursor uses only `--resume` (fresh chats omit session flags).
    pub session: Option<SessionMode>,
    /// Working directory for the subprocess (default: inherit from parent).
    pub working_dir: Option<PathBuf>,
    /// When true, print the command and cwd to stderr before running.
    pub debug: bool,
    /// When true, emit raw agent output. If agent_output_sink is set, routes there; else prints to stderr.
    pub agent_output: bool,
    /// When set and agent_output is true, routes output here instead of stderr (for TUI).
    pub agent_output_sink: Option<AgentOutputSink>,
    /// When set, routes progress events (ToolUse, TaskStarted, TaskProgress) here instead of instance callback.
    pub progress_sink: Option<ProgressSink>,
    /// When set, write entire agent conversation (raw bytes from stdout) to this file.
    pub conversation_output_path: Option<PathBuf>,
    /// When true, inherit stdin so the user can grant permission prompts interactively.
    pub inherit_stdin: bool,
    /// Extra tools to add to the goal's allowlist (backends that support allowlists merge these).
    pub extra_allowed_tools: Option<Vec<String>>,
    /// When set, backend sets TDDY_SOCKET env var for tddy-tools relay.
    pub socket_path: Option<PathBuf>,
    /// When set, backend sets TDDY_SESSION_DIR and TDDY_REPO_DIR for tddy-tools path pre-allow.
    pub session_dir: Option<PathBuf>,
    /// When set, backend exports TDDY_REMOTE_* env vars for remote-codebase mode routing.
    pub remote: Option<RemoteToolEnv>,
}

impl Default for InvokeRequest {
    fn default() -> Self {
        use crate::workflow::recipe::{GoalHints, PermissionHint};
        Self {
            prompt: String::new(),
            system_prompt: None,
            system_prompt_path: None,
            goal_id: GoalId::new(""),
            submit_key: GoalId::new(""),
            hints: GoalHints {
                display_name: String::new(),
                permission: PermissionHint::ReadOnly,
                allowed_tools: Vec::new(),
                default_model: None,
                agent_output: false,
                agent_cli_plan_mode: false,
                claude_nonzero_exit_ok_if_structured_response: false,
            },
            model: None,
            session: None,
            working_dir: None,
            debug: false,
            agent_output: false,
            agent_output_sink: None,
            progress_sink: None,
            conversation_output_path: None,
            inherit_stdin: false,
            extra_allowed_tools: None,
            socket_path: None,
            session_dir: None,
            remote: None,
        }
    }
}

fn default_allow_other() -> bool {
    true
}

/// Build a PATH that prepends the directory of the current executable.
/// This ensures `tddy-tools` (built alongside `tddy-coder`) is discoverable
/// by agents that call it as a bare command.
pub(crate) fn path_with_exe_dir() -> std::ffi::OsString {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
        }
    }
    if let Some(existing) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&existing) {
            if !dirs.contains(&p) {
                dirs.push(p);
            }
        }
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

/// Structured clarification question from AskUserQuestion tool.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClarificationQuestion {
    pub header: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    #[serde(default, alias = "multiSelect")]
    pub multi_select: bool,
    /// When false, omit "Other (type your own)" — e.g. for binary permission (Yes/No).
    #[serde(default = "default_allow_other")]
    pub allow_other: bool,
}

/// Option for a clarification question.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QuestionOption {
    pub label: String,
    /// Secondary line in the TUI; omit in JSON when unused (`tddy-tools ask`).
    #[serde(default)]
    pub description: String,
}

/// Build a clarification question for interactive coding backend selection at session start.
#[must_use]
pub fn backend_selection_question() -> ClarificationQuestion {
    ClarificationQuestion {
        header: "Backend".to_string(),
        question: "Select the coding backend".to_string(),
        options: vec![
            QuestionOption {
                label: "Claude".to_string(),
                description: "Claude Code CLI (default model: opus)".to_string(),
            },
            QuestionOption {
                label: "Claude ACP".to_string(),
                description: "Claude Agent Control Protocol (default model: opus)".to_string(),
            },
            QuestionOption {
                label: "Cursor".to_string(),
                description: "Cursor agent CLI (default model: composer-2.5)".to_string(),
            },
            QuestionOption {
                label: "Codex".to_string(),
                description: "OpenAI Codex CLI (default model: gpt-5)".to_string(),
            },
            QuestionOption {
                label: "Codex ACP".to_string(),
                description: "OpenAI Codex via codex-acp (default model: gpt-5)".to_string(),
            },
            QuestionOption {
                label: "Stub".to_string(),
                description: "Test backend with simulated responses".to_string(),
            },
        ],
        multi_select: false,
        allow_other: false,
    }
}

/// Single-select question for switching workflow recipe after `/recipe` from the feature slash menu.
#[must_use]
pub fn workflow_recipe_selection_question() -> ClarificationQuestion {
    ClarificationQuestion {
        header: "Workflow recipe".to_string(),
        question: "Select the workflow recipe for this session".to_string(),
        options: vec![
            QuestionOption {
                label: "TDD".to_string(),
                description: "Plan → red → green → refactor cycle".to_string(),
            },
            QuestionOption {
                label: "Bugfix".to_string(),
                description: "Reproduce → fix workflow".to_string(),
            },
            QuestionOption {
                label: "Free prompting".to_string(),
                description: "Open-ended agent loop without PRD/TDD gates".to_string(),
            },
            QuestionOption {
                label: "Grill me".to_string(),
                description: "Grill (questions) then Create plan (grill-me-brief.md)".to_string(),
            },
            QuestionOption {
                label: "Plan PR stack".to_string(),
                description: "Analyze feature intent and emit a structured PR-stack plan"
                    .to_string(),
            },
            QuestionOption {
                label: "Orchestrate PR stack".to_string(),
                description: "Resumable loop that merges a PR stack to master".to_string(),
            },
        ],
        multi_select: false,
        allow_other: false,
    }
}

/// Map a [`workflow_recipe_selection_question`] option label to CLI recipe name.
#[must_use]
pub fn recipe_cli_name_from_selection_label(label: &str) -> Option<&'static str> {
    match label {
        "TDD" => Some("tdd"),
        "Bugfix" => Some("bugfix"),
        "Free prompting" => Some("free-prompting"),
        "Grill me" => Some("grill-me"),
        "Plan PR stack" => Some("plan-pr-stack"),
        "Orchestrate PR stack" => Some("orchestrate-pr-stack"),
        _ => None,
    }
}

/// Map a display label from [`backend_selection_question`] to `(agent_name, default_model)`.
#[must_use]
pub fn backend_from_label(label: &str) -> (&'static str, &'static str) {
    match label {
        "Claude" => ("claude", "opus"),
        "Claude ACP" => ("claude-acp", "opus"),
        "Cursor" => ("cursor", "composer-2.5"),
        "Codex" => ("codex", "gpt-5"),
        "Codex ACP" => ("codex-acp", "gpt-5"),
        "Stub" => ("stub", "stub"),
        _ => ("claude", "opus"),
    }
}

/// Default model name for a given agent identifier (e.g. `claude`, `cursor`).
#[must_use]
pub fn default_model_for_agent(agent: &str) -> &'static str {
    match agent {
        "cursor" => "composer-2.5",
        "codex" => "gpt-5",
        "codex-acp" => "gpt-5",
        "stub" => "stub",
        _ => "opus",
    }
}

/// A model a backend can run with, for UI selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendModel {
    /// Value passed to the backend as `--model` (e.g. `"opus"`, `"gpt-5.2"`, `"claude-opus-4-8"`).
    pub id: String,
    /// Human-readable label (e.g. `"Claude Opus"`, `"GPT-5.2"`).
    pub label: String,
}

impl BackendModel {
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// A backend's selectable models plus the id to preselect (its current/default model).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackendModels {
    pub models: Vec<BackendModel>,
    pub default_model: String,
}

/// Curated model list for backends whose command cannot enumerate models (`claude`, `codex`).
/// Single source of truth for those catalogs; kept in sync with [`default_model_for_agent`].
#[must_use]
pub fn curated_models_for_agent(agent: &str) -> BackendModels {
    let (models, default_model): (&[(&str, &str)], &str) = match agent {
        "codex" | "codex-acp" => (&[("gpt-5", "GPT-5")], "gpt-5"),
        "cursor" => (&[("composer-2.5", "Composer 2.5")], "composer-2.5"),
        "stub" => (&[("stub", "Stub")], "stub"),
        _ => (
            &[
                ("opus", "Claude Opus"),
                ("sonnet", "Claude Sonnet"),
                ("haiku", "Claude Haiku"),
            ],
            "opus",
        ),
    };
    BackendModels {
        models: models
            .iter()
            .map(|(id, label)| BackendModel::new(*id, *label))
            .collect(),
        default_model: default_model.to_string(),
    }
}

/// Curated model catalog for the `claude-cli` session type (full Claude ids passed to `claude
/// --model`). Single source of truth (replaces the web `CLAUDE_CLI_MODELS` constant).
#[must_use]
pub fn claude_cli_models() -> BackendModels {
    BackendModels {
        models: vec![
            BackendModel::new("claude-opus-4-8", "Claude Opus 4.8"),
            BackendModel::new("claude-sonnet-4-6", "Claude Sonnet 4.6"),
            BackendModel::new("claude-haiku-4-5-20251001", "Claude Haiku 4.5"),
        ],
        default_model: "claude-opus-4-8".to_string(),
    }
}

/// Curated model catalog for the `cursor-cli` session type (ids passed to `agent --model`).
#[must_use]
pub fn cursor_cli_models() -> BackendModels {
    BackendModels {
        models: vec![
            BackendModel::new("gpt-5.3-codex", "GPT-5.3 Codex"),
            BackendModel::new(
                "claude-4.6-sonnet-medium-thinking",
                "Claude 4.6 Sonnet (thinking)",
            ),
            BackendModel::new(
                "claude-sonnet-5-thinking-high",
                "Claude Sonnet 5 (thinking high)",
            ),
            BackendModel::new("composer-2.5", "Composer 2.5"),
            BackendModel::new("glm-5.2-high", "GLM 5.2 High"),
        ],
        default_model: "claude-4.6-sonnet-medium-thinking".to_string(),
    }
}

/// Map an ACP agent's advertised [`agent_client_protocol::SessionModelState`] into [`BackendModels`].
/// Errors when the agent advertised no models (an unavailable backend must not look available).
pub fn acp_models_from_session_state(
    state: Option<&agent_client_protocol::SessionModelState>,
) -> Result<BackendModels, BackendError> {
    let state = state.ok_or_else(|| {
        BackendError::InvocationFailed("agent advertised no session model state".to_string())
    })?;
    if state.available_models.is_empty() {
        return Err(BackendError::InvocationFailed(
            "agent advertised no available models".to_string(),
        ));
    }
    Ok(BackendModels {
        models: state
            .available_models
            .iter()
            .map(|m| BackendModel::new(m.model_id.to_string(), m.name.clone()))
            .collect(),
        default_model: state.current_model_id.to_string(),
    })
}

/// Index into [`backend_selection_question`] options for a given agent name.
#[must_use]
pub fn preselected_index_for_agent(agent: &str) -> usize {
    match agent {
        "claude" => 0,
        "claude-acp" => 1,
        "cursor" => 2,
        "codex" => 3,
        "codex-acp" => 4,
        "stub" => 5,
        _ => 0,
    }
}

/// Response from the coding backend.
#[derive(Debug, Clone)]
pub struct InvokeResponse {
    pub output: String,
    pub exit_code: i32,
    /// Session/thread ID for resume; None when backend does not support or provide one.
    pub session_id: Option<String>,
    pub questions: Vec<ClarificationQuestion>,
    /// Raw stream lines from agent stdout, for debugging when output parsing fails.
    pub raw_stream: Option<String>,
    /// Stderr from the subprocess, for debugging when output is empty.
    pub stderr: Option<String>,
}

/// Trait for LLM-based coding backends.
#[async_trait::async_trait]
pub trait CodingBackend: Send + Sync {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError>;
    /// Backend identifier (e.g. "claude", "cursor", "mock") for changeset and display.
    fn name(&self) -> &str;
    /// Per-instance submit result channel. Backends using InMemoryToolExecutor
    /// return their channel here so tasks can read without touching global state.
    fn submit_channel(&self) -> Option<&crate::toolcall::SubmitResultChannel> {
        None
    }

    /// When **`false`**, workflow tasks must call [`CodingBackend::invoke`] for every backend step,
    /// even when a session action-cache entry would allow a replay.
    ///
    /// [`crate::workflow::task::BackendInvokeTask`] uses this to avoid skipping [`MockBackend`]
    /// invocations — the mock advances a FIFO response queue synchronized with submits.
    fn action_invoke_cache_eligible(&self) -> bool {
        true
    }

    /// Enumerate the models this backend can run with, for UI selection. The default returns the
    /// curated catalog for the backend's [`CodingBackend::name`]; backends whose command can
    /// enumerate its own models (cursor, ACP) override this to query the agent at runtime.
    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        Ok(curated_models_for_agent(self.name()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize tests that mutate global CHILD_PID.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        CHILD_PID.store(0, Ordering::SeqCst);
        guard
    }

    #[test]
    fn set_child_pid_stores_pid() {
        let _lock = lock_and_reset();
        set_child_pid(12345);
        assert_eq!(get_child_pid(), 12345);
    }

    #[test]
    fn clear_child_pid_resets_to_zero() {
        let _lock = lock_and_reset();
        set_child_pid(99999);
        clear_child_pid();
        assert_eq!(get_child_pid(), 0);
    }

    fn ids(models: &BackendModels) -> Vec<&str> {
        models.models.iter().map(|m| m.id.as_str()).collect()
    }

    #[test]
    fn curated_claude_models_offer_opus_sonnet_and_haiku_defaulting_to_opus() {
        // When
        let catalog = curated_models_for_agent("claude");

        // Then
        assert_eq!(ids(&catalog), vec!["opus", "sonnet", "haiku"]);
        assert_eq!(catalog.default_model, "opus");
    }

    #[test]
    fn curated_codex_models_offer_gpt5_defaulting_to_gpt5() {
        // When
        let catalog = curated_models_for_agent("codex");

        // Then
        assert_eq!(ids(&catalog), vec!["gpt-5"]);
        assert_eq!(catalog.default_model, "gpt-5");
    }

    #[test]
    fn claude_cli_models_offer_the_full_claude_ids_defaulting_to_opus_4_8() {
        // When
        let catalog = claude_cli_models();

        // Then
        assert_eq!(
            ids(&catalog),
            vec![
                "claude-opus-4-8",
                "claude-sonnet-4-6",
                "claude-haiku-4-5-20251001"
            ]
        );
        assert_eq!(catalog.default_model, "claude-opus-4-8");
    }

    #[test]
    fn acp_session_state_maps_available_models_with_the_current_one_as_default() {
        // Given — the agent advertises two models and names the current one
        use agent_client_protocol::{ModelInfo, SessionModelState};
        let state = SessionModelState::new(
            "gpt-5.2",
            vec![
                ModelInfo::new("auto", "Auto"),
                ModelInfo::new("gpt-5.2", "GPT-5.2"),
            ],
        );

        // When
        let catalog = acp_models_from_session_state(Some(&state)).expect("should map models");

        // Then
        assert_eq!(ids(&catalog), vec!["auto", "gpt-5.2"]);
        assert_eq!(catalog.default_model, "gpt-5.2");
    }

    #[test]
    fn acp_enumeration_errors_when_the_agent_advertises_no_models() {
        // When / Then — no SessionModelState at all is an error, not an empty list
        let result = acp_models_from_session_state(None);

        assert!(matches!(result, Err(BackendError::InvocationFailed(_))));
    }

    #[test]
    fn kill_child_process_returns_false_when_no_child() {
        let _lock = lock_and_reset();
        assert!(!kill_child_process());
    }

    #[cfg(unix)]
    #[test]
    fn kill_child_process_kills_running_child() {
        let _lock = lock_and_reset();

        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("failed to spawn sleep");
        let pid = child.id();
        set_child_pid(pid);

        assert!(kill_child_process());
        assert_eq!(get_child_pid(), 0);

        // Reap the child so it doesn't remain a zombie, then verify it was killed.
        let status = child.wait().expect("failed to wait on child");
        assert!(!status.success());
    }

    #[test]
    fn backend_selection_question_returns_six_options_including_codex_variants() {
        let q = backend_selection_question();
        assert_eq!(q.options.len(), 6);
        assert!(!q.multi_select);
        assert!(!q.allow_other);
    }

    #[test]
    fn codex_backend_selection_question_labels_order() {
        let q = backend_selection_question();
        let labels: Vec<&str> = q.options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Claude",
                "Claude ACP",
                "Cursor",
                "Codex",
                "Codex ACP",
                "Stub"
            ]
        );
    }

    #[test]
    fn backend_selection_includes_codex_option() {
        let q = backend_selection_question();
        let codex = q
            .options
            .iter()
            .find(|o| o.label == "Codex")
            .expect("Codex option must be present for codex agent support");
        assert!(
            codex.description.to_lowercase().contains("codex"),
            "Codex option should describe Codex CLI, got {:?}",
            codex.description
        );
    }

    #[test]
    fn backend_from_label_claude() {
        assert_eq!(backend_from_label("Claude"), ("claude", "opus"));
    }

    #[test]
    fn backend_from_label_cursor() {
        assert_eq!(backend_from_label("Cursor"), ("cursor", "composer-2.5"));
    }

    #[test]
    fn backend_from_label_claude_acp() {
        assert_eq!(backend_from_label("Claude ACP"), ("claude-acp", "opus"));
    }

    #[test]
    fn backend_from_label_stub() {
        assert_eq!(backend_from_label("Stub"), ("stub", "stub"));
    }

    #[test]
    fn backend_from_label_codex() {
        assert_eq!(backend_from_label("Codex"), ("codex", "gpt-5"));
    }

    #[test]
    fn backend_from_label_codex_acp() {
        assert_eq!(backend_from_label("Codex ACP"), ("codex-acp", "gpt-5"));
    }

    #[test]
    fn backend_from_label_unknown_defaults_to_claude() {
        assert_eq!(backend_from_label("Unknown"), ("claude", "opus"));
    }

    #[test]
    fn default_model_for_agent_cursor() {
        assert_eq!(default_model_for_agent("cursor"), "composer-2.5");
    }

    #[test]
    fn default_model_for_agent_codex() {
        assert_eq!(default_model_for_agent("codex"), "gpt-5");
    }

    #[test]
    fn default_model_for_agent_codex_acp() {
        assert_eq!(default_model_for_agent("codex-acp"), "gpt-5");
    }

    #[test]
    fn default_model_for_agent_claude() {
        assert_eq!(default_model_for_agent("claude"), "opus");
    }

    #[test]
    fn codex_preselected_index_for_agent_order() {
        assert_eq!(preselected_index_for_agent("claude"), 0);
        assert_eq!(preselected_index_for_agent("claude-acp"), 1);
        assert_eq!(preselected_index_for_agent("cursor"), 2);
        assert_eq!(preselected_index_for_agent("codex"), 3);
        assert_eq!(preselected_index_for_agent("codex-acp"), 4);
        assert_eq!(preselected_index_for_agent("stub"), 5);
        assert_eq!(preselected_index_for_agent("unknown"), 0);
    }
}
