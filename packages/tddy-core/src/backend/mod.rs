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

/// Workflow goal; backends map this to their own permission/session model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    Plan,
    AcceptanceTests,
    Red,
    Green,
    Validate,
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
