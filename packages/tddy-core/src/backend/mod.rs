//! Coding backend abstraction for LLM-based coders.

mod claude;
mod cursor;
mod mock;

pub use claude::{build_claude_args, ClaudeCodeBackend, ClaudeInvokeConfig, PermissionMode};
pub use cursor::CursorBackend;
pub use mock::MockBackend;

/// Enum dispatch for CLI backend selection (avoids trait object overhead).
#[derive(Debug)]
pub enum AnyBackend {
    Claude(ClaudeCodeBackend),
    Cursor(CursorBackend),
}

impl CodingBackend for AnyBackend {
    fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        match self {
            AnyBackend::Claude(b) => b.invoke(request),
            AnyBackend::Cursor(b) => b.invoke(request),
        }
    }

    fn name(&self) -> &str {
        match self {
            AnyBackend::Claude(b) => b.name(),
            AnyBackend::Cursor(b) => b.name(),
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

/// Non-unix stub: clears the tracked PID but cannot actually kill the process.
#[cfg(not(unix))]
pub fn kill_child_process() -> bool {
    let pid = CHILD_PID.swap(0, Ordering::SeqCst);
    if pid == 0 {
        return false;
    }
    eprintln!(
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
    Validate,
    /// Renamed/replacement for Validate: analyze git changes and produce an evaluation report.
    Evaluate,
    /// Orchestrate validate-tests, validate-prod-ready, and analyze-clean-code subagents.
    ValidateRefactor,
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
    /// When true, print raw agent output to stderr in real-time.
    pub agent_output: bool,
    /// When set, write entire agent conversation (raw bytes from stdout) to this file.
    pub conversation_output_path: Option<PathBuf>,
    /// When true, inherit stdin so the user can grant permission prompts interactively.
    pub inherit_stdin: bool,
    /// Extra tools to add to the goal's allowlist (backends that support allowlists merge these).
    pub extra_allowed_tools: Option<Vec<String>>,
}

/// Structured clarification question from AskUserQuestion tool.
#[derive(Debug, Clone)]
pub struct ClarificationQuestion {
    pub header: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

/// Option for a clarification question.
#[derive(Debug, Clone)]
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
pub trait CodingBackend: Send + Sync {
    fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError>;
    /// Backend identifier (e.g. "claude", "cursor", "mock") for changeset and display.
    fn name(&self) -> &str;
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
