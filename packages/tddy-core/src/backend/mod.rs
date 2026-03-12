//! Coding backend abstraction for LLM-based coders.

mod claude;
mod cursor;
mod mock;
mod stub;
mod tool_executor;

pub use claude::{build_claude_args, ClaudeCodeBackend, ClaudeInvokeConfig, PermissionMode};
pub use cursor::CursorBackend;
pub use mock::MockBackend;
pub use stub::StubBackend;
pub use tool_executor::{InMemoryToolExecutor, ProcessToolExecutor, ToolExecutor};

/// Enum dispatch for CLI backend selection (avoids trait object overhead).
/// tddy-coder uses claude/cursor only. tddy-demo uses stub (via lib, not CLI).
#[derive(Debug)]
pub enum AnyBackend {
    Claude(ClaudeCodeBackend),
    Cursor(CursorBackend),
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
            AnyBackend::Cursor(b) => b.invoke(request).await,
            AnyBackend::Stub(b) => b.invoke(request).await,
        }
    }

    fn name(&self) -> &str {
        match self {
            AnyBackend::Claude(b) => b.name(),
            AnyBackend::Cursor(b) => b.name(),
            AnyBackend::Stub(b) => b.name(),
        }
    }

    fn submit_channel(&self) -> Option<&crate::toolcall::SubmitResultChannel> {
        match self {
            AnyBackend::Claude(b) => b.submit_channel(),
            AnyBackend::Cursor(b) => b.submit_channel(),
            AnyBackend::Stub(b) => b.submit_channel(),
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
                &arg[..arg.len().min(max_arg_len)],
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

/// Workflow goal; backends map this to their own permission/session model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    Plan,
    AcceptanceTests,
    Red,
    Green,
    /// Standalone demo execution step.
    Demo,
    /// Renamed/replacement for Validate: analyze git changes and produce an evaluation report.
    Evaluate,
    /// Orchestrate validate-tests, validate-prod-ready, and analyze-clean-code subagents.
    Validate,
    /// Execute refactoring plan from refactoring-plan.md.
    Refactor,
    /// Update repo documentation from PRD, changeset, progress per repo guidelines.
    UpdateDocs,
}

impl Goal {
    /// Key used for store_submit_result / take_submit_result_for_goal (matches JSON "goal" field).
    pub fn submit_key(&self) -> &'static str {
        match self {
            Goal::Plan => "plan",
            Goal::AcceptanceTests => "acceptance-tests",
            Goal::Red => "red",
            Goal::Green => "green",
            Goal::Demo => "demo",
            Goal::Evaluate => "evaluate-changes",
            Goal::Validate => "validate",
            Goal::Refactor => "refactor",
            Goal::UpdateDocs => "update-docs",
        }
    }
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

/// Request to invoke the coding backend.
#[derive(Debug, Clone)]
pub struct InvokeRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    /// When set, backend uses this path instead of system_prompt (avoids temp file).
    pub system_prompt_path: Option<PathBuf>,
    pub goal: Goal,
    /// Optional model name (e.g. "sonnet") passed to the agent.
    pub model: Option<String>,
    /// Session/thread ID for resume (first call: None; followup: Some(id)).
    pub session_id: Option<String>,
    /// When true, use --resume instead of --session-id (or equivalent).
    pub is_resume: bool,
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
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
}
