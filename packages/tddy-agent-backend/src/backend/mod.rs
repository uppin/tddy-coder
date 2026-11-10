//! Coding backend abstraction for LLM-based coders.

pub mod acp_driver;
mod claude;
pub(crate) mod codex;
pub mod codex_acp;
pub mod headless_driver;
mod mock;
pub mod model_catalog;
mod routed;
pub mod spec_backend;
pub mod spec_headless;
pub use spec_headless::SpecHeadlessBackend;
pub(crate) mod turn_process;
pub use routed::toml_route_overrides;
pub use turn_process::{kill_running_turns, RunningGroup};
mod stub;
mod tool_executor;

pub use claude::{read_claude_subagent_usages, read_claude_transcript_usage};
pub use codex::write_codex_thread_id_file;
pub use codex::{
    spawn_codex_oauth_login, CODEX_DEFAULT_CLI_BINARY, CODEX_OAUTH_AUTHORIZE_URL_FILENAME,
    CODEX_THREAD_ID_FILENAME,
};
pub use mock::MockBackend;
pub use model_catalog::render_models_json;
pub use stub::StubBackend;
pub use tddy_workflow::questions::{ClarificationQuestion, QuestionOption};
use tddy_workflow::{GoalHints, GoalId};
pub use tool_executor::{InMemoryToolExecutor, ProcessToolExecutor, ToolExecutor};

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
    /// Create SharedBackend from an Arc<dyn CodingBackend> (e.g. for MockBackend in tests).
    pub fn from_arc(inner: std::sync::Arc<dyn CodingBackend>) -> Self {
        Self(inner)
    }

    /// Get the inner Arc for use with graph builders that require Arc<dyn CodingBackend>.
    pub fn as_arc(&self) -> std::sync::Arc<dyn CodingBackend> {
        self.0.clone()
    }
}

use crate::error::BackendError;
use std::path::PathBuf;
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

/// Gather the full per-conversation token-usage snapshot for a session by merging every source,
/// in a stable order: when `include_main_agent`, the main Claude agent's own transcript usage
/// ([`read_claude_transcript_usage`]) followed by its nested Task-tool subagents
/// ([`read_claude_subagent_usages`]); then the tddy subagent conversations the in-jail MCP server
/// wrote to `<session_dir>/egress/accounting.json`.
///
/// Best-effort on the accounting file: a missing or unreadable file simply contributes no rows,
/// never an error. With no transcript and no accounting file, `include_main_agent` still yields a
/// single zero-token main-agent row carrying `fallback_model`, so a caller always has something to
/// render.
pub fn gather_session_usage(
    session_dir: &std::path::Path,
    session_id: &str,
    claude_home: &std::path::Path,
    fallback_model: &str,
    include_main_agent: bool,
) -> Vec<crate::token_accounting::ConversationRecord> {
    use crate::token_accounting::ConversationRecord;

    #[derive(serde::Deserialize)]
    struct AccountingFile {
        #[serde(default)]
        conversations: Vec<ConversationRecord>,
    }

    let mut records = Vec::new();
    if include_main_agent {
        records.push(read_claude_transcript_usage(
            claude_home,
            session_id,
            fallback_model,
        ));
        records.extend(read_claude_subagent_usages(
            claude_home,
            session_id,
            fallback_model,
        ));
    }

    let accounting_path = session_dir.join("egress").join("accounting.json");
    if let Ok(text) = std::fs::read_to_string(&accounting_path) {
        if let Ok(parsed) = serde_json::from_str::<AccountingFile>(&text) {
            records.extend(parsed.conversations);
        }
    }

    records
}

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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Unix socket to reach the daemon on instead of `daemon_url`, for a host that serves no HTTP
    /// listener at all — an application embedding the daemon in its own process (Tddy Desktop).
    /// A co-located agent is a separate process, so it can use neither the application's in-process
    /// bridge nor `listen.web_port`, which on such a host answers only the OAuth callback.
    pub daemon_socket: Option<String>,
    pub session_id: String,
    pub session_token: String,
    pub daemon_instance_id: Option<String>,
    pub livekit_url: Option<String>,
    pub livekit_room: Option<String>,
    pub server_identity: Option<String>,
    /// Scoped LiveKit join token minted by the spawning daemon, for a split session whose worktree
    /// lives on another daemon (docs/ft/daemon/remote-managed-worktree.md). Never the daemon's
    /// `livekit.api_secret`, which would let the agent join any room as any identity.
    pub livekit_token: Option<String>,
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
        if let Some(v) = &self.daemon_socket {
            pairs.push(("TDDY_REMOTE_DAEMON_SOCKET".to_string(), v.clone()));
        }
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
        if let Some(v) = &self.livekit_token {
            pairs.push(("TDDY_REMOTE_LIVEKIT_TOKEN".to_string(), v.clone()));
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
        use tddy_workflow::{GoalHints, PermissionHint};
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

/// A model a backend can run with, for UI selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendModel {
    /// Value passed to the backend as `--model` (e.g. `"opus"`, `"gpt-5.2"`, `"claude-opus-5"`).
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

/// The model catalogue an ACP agent's session state advertises: its available models, with the
/// current one as the default.
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

    /// Enumerate the models this backend can run with, for UI selection. The default is an
    /// empty catalogue: a backend's models are its spec's `coding_agent.models`, supplied by the
    /// caller that built it from that spec; backends whose command enumerates its own models
    /// override this to query the agent at runtime.
    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        Ok(BackendModels::default())
    }
}

#[cfg(test)]
mod gather_session_usage_tests {
    use super::gather_session_usage;
    use crate::token_accounting::ConversationRecord;
    use std::path::Path;

    /// One `type:"assistant"` transcript line carrying a single turn's input/output usage.
    fn assistant_line(model: &str, input: u64, output: u64) -> String {
        serde_json::json!({
            "type": "assistant",
            "message": { "model": model, "usage": { "input_tokens": input, "output_tokens": output } },
        })
        .to_string()
    }

    /// Write the main Claude transcript at the deterministic
    /// `<claude_home>/.claude/projects/<proj>/<session_id>.jsonl` path (one assistant turn).
    fn write_main_transcript(
        claude_home: &Path,
        session_id: &str,
        model: &str,
        input: u64,
        output: u64,
    ) {
        let dir = claude_home.join(".claude").join("projects").join("proj");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{session_id}.jsonl")),
            format!("{}\n", assistant_line(model, input, output)),
        )
        .unwrap();
    }

    /// Write a Task-tool subagent transcript + its `agentType` meta sibling.
    fn write_subagent(
        claude_home: &Path,
        session_id: &str,
        agent_stem: &str,
        agent_type: &str,
        model: &str,
        input: u64,
        output: u64,
    ) {
        let dir = claude_home
            .join(".claude")
            .join("projects")
            .join("proj")
            .join(session_id)
            .join("subagents");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{agent_stem}.jsonl")),
            format!("{}\n", assistant_line(model, input, output)),
        )
        .unwrap();
        std::fs::write(
            dir.join(format!("{agent_stem}.meta.json")),
            serde_json::json!({ "agentType": agent_type }).to_string(),
        )
        .unwrap();
    }

    /// Write the in-jail accounting file the tddy MCP server produces (camelCase token fields).
    fn write_accounting(session_dir: &Path) {
        let egress = session_dir.join("egress");
        std::fs::create_dir_all(&egress).unwrap();
        std::fs::write(
            egress.join("accounting.json"),
            serde_json::json!({
                "conversations": [{
                    "agent": "explorer",
                    "id": "fc-1",
                    "model": "ollama",
                    "inputTokens": 5,
                    "outputTokens": 1,
                    "totalTokens": 6,
                    "turns": 1,
                }]
            })
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn merges_main_agent_its_task_subagents_and_the_accounting_file_in_order() {
        // Given a session with a main transcript, one Task subagent, and a tddy accounting file
        let tmp = tempfile::tempdir().unwrap();
        let claude_home = tmp.path().join("home");
        let session_dir = tmp.path().join("session");
        write_main_transcript(&claude_home, "sess-1", "claude-opus-4-8", 100, 20);
        write_subagent(
            &claude_home,
            "sess-1",
            "agent-01",
            "Explore",
            "claude-haiku-4-5",
            40,
            8,
        );
        write_accounting(&session_dir);

        // When
        let records =
            gather_session_usage(&session_dir, "sess-1", &claude_home, "fallback-model", true);

        // Then — main agent, then its subagent, then the tddy-subagent row
        assert_eq!(
            records,
            vec![
                ConversationRecord {
                    agent: "claude".to_string(),
                    id: "sess-1".to_string(),
                    model: "claude-opus-4-8".to_string(),
                    input_tokens: 100,
                    output_tokens: 20,
                    total_tokens: 120,
                    turns: 1,
                },
                ConversationRecord {
                    agent: "Explore".to_string(),
                    id: "agent-01".to_string(),
                    model: "claude-haiku-4-5".to_string(),
                    input_tokens: 40,
                    output_tokens: 8,
                    total_tokens: 48,
                    turns: 1,
                },
                ConversationRecord {
                    agent: "explorer".to_string(),
                    id: "fc-1".to_string(),
                    model: "ollama".to_string(),
                    input_tokens: 5,
                    output_tokens: 1,
                    total_tokens: 6,
                    turns: 1,
                },
            ]
        );
    }

    #[test]
    fn omits_the_main_agent_and_its_subagents_when_include_main_agent_is_false() {
        // Given a session that has a Claude transcript on disk, but only the tddy accounting file
        // should be reported (e.g. a Cursor session — no Claude main-agent row)
        let tmp = tempfile::tempdir().unwrap();
        let claude_home = tmp.path().join("home");
        let session_dir = tmp.path().join("session");
        write_main_transcript(&claude_home, "sess-2", "claude-opus-4-8", 100, 20);
        write_accounting(&session_dir);

        // When
        let records = gather_session_usage(
            &session_dir,
            "sess-2",
            &claude_home,
            "fallback-model",
            false,
        );

        // Then — only the accounting conversation, no claude/subagent rows
        assert_eq!(
            records,
            vec![ConversationRecord {
                agent: "explorer".to_string(),
                id: "fc-1".to_string(),
                model: "ollama".to_string(),
                input_tokens: 5,
                output_tokens: 1,
                total_tokens: 6,
                turns: 1,
            }]
        );
    }

    #[test]
    fn reports_a_single_zero_token_main_agent_row_with_the_fallback_model_when_no_transcript_exists(
    ) {
        // Given an empty home and no accounting file
        let tmp = tempfile::tempdir().unwrap();

        // When
        let records = gather_session_usage(
            &tmp.path().join("session"),
            "sess-x",
            &tmp.path().join("home"),
            "fallback-model",
            true,
        );

        // Then — one zero-token main-agent row (never an error)
        assert_eq!(
            records,
            vec![ConversationRecord {
                agent: "claude".to_string(),
                id: "sess-x".to_string(),
                model: "fallback-model".to_string(),
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                turns: 0,
            }]
        );
    }
}
