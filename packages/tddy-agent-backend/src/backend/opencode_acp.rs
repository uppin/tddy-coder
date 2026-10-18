//! OpenCode as a tool-session agent: `opencode acp` driven over the Agent Client Protocol, the same
//! way [`super::ClaudeAcpBackend`] drives `claude-agent-acp`.

use std::path::PathBuf;

use async_trait::async_trait;

use super::{BackendModels, CodingBackend, InvokeRequest, InvokeResponse};
use crate::error::BackendError;

/// `opencode acp` behind [`CodingBackend`].
#[derive(Debug)]
pub struct OpenCodeAcpBackend {
    agent_path: PathBuf,
}

impl OpenCodeAcpBackend {
    /// Default executable on `PATH`.
    pub const DEFAULT_CLI_BINARY: &'static str = "opencode";

    #[must_use]
    pub fn new() -> Self {
        Self::with_agent_path(PathBuf::from(Self::DEFAULT_CLI_BINARY))
    }

    /// Run the given binary (`opencode`, or a stub agent in tests) as `<binary> acp`.
    #[must_use]
    pub fn with_agent_path(path: PathBuf) -> Self {
        Self { agent_path: path }
    }
}

impl Default for OpenCodeAcpBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CodingBackend for OpenCodeAcpBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let _ = (request, &self.agent_path);
        // TODO(opencode-acp): implement
        todo!()
    }

    fn name(&self) -> &str {
        "opencode"
    }

    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        // TODO(opencode-acp): implement
        todo!()
    }
}
