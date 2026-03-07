//! Coding backend abstraction for LLM-based coders.

mod claude;
mod mock;

pub use claude::{build_claude_args, ClaudeCodeBackend};
pub use mock::MockBackend;

use crate::error::BackendError;

/// Request to invoke the coding backend.
#[derive(Debug, Clone)]
pub struct InvokeRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub permission_mode: PermissionMode,
    /// Optional model name (e.g. "sonnet") passed as --model to Claude Code CLI.
    pub model: Option<String>,
    /// Session ID for --session-id (first call) or --resume (followup).
    pub session_id: Option<String>,
    /// When true, use --resume instead of --session-id.
    pub is_resume: bool,
    /// When true, print raw agent output to stderr in real-time.
    pub agent_output: bool,
    /// When true, inherit stdin so the user can grant permission prompts interactively.
    pub inherit_stdin: bool,
    /// Optional list of tools to auto-allow via --allowedTools (e.g. Read, Write, Bash(cargo *)).
    pub allowed_tools: Option<Vec<String>>,
    /// Optional permission prompt tool name for --permission-prompt-tool (e.g. approval_prompt).
    pub permission_prompt_tool: Option<String>,
    /// Optional path to MCP config for --mcp-config when using permission prompt tool.
    pub mcp_config_path: Option<std::path::PathBuf>,
    /// Working directory for the subprocess (default: inherit from parent).
    pub working_dir: Option<std::path::PathBuf>,
    /// When true, print the command and cwd to stderr before running.
    pub debug: bool,
}

/// Permission mode for the backend (e.g. plan = read-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// Read-only analysis, no file edits or Bash.
    Plan,
    /// Standard behavior: prompts for permission on first use of each tool.
    Default,
    /// Automatically accepts file edit permissions for the session.
    AcceptEdits,
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
    pub session_id: String,
    pub questions: Vec<ClarificationQuestion>,
    /// Raw NDJSON stream lines from Claude CLI stdout, for debugging when output parsing fails.
    pub raw_stream: Option<String>,
    /// Stderr from the subprocess, for debugging when output is empty.
    pub stderr: Option<String>,
}

/// Trait for LLM-based coding backends.
pub trait CodingBackend: Send + Sync {
    fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError>;
}
