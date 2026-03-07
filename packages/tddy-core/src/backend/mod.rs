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
}

/// Permission mode for the backend (e.g. plan = read-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Plan,
    Default,
}

/// Response from the coding backend.
#[derive(Debug, Clone)]
pub struct InvokeResponse {
    pub output: String,
    pub exit_code: i32,
}

/// Trait for LLM-based coding backends.
pub trait CodingBackend: Send + Sync {
    fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError>;
}
