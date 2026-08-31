//! Synchronous LSP transport over an existing [`LspClient`].
//!
//! [`RustBackend::from_lsp_client`] stores a long-running client from `tddy-lsp` rather than
//! spawning rust-analyzer itself. The backend's request loop is synchronous; this bridge blocks
//! the current tokio runtime handle on the client's async methods.

use crate::{RestructureError, Result};
use serde_json::Value;
use std::sync::Arc;
use tddy_lsp::client::LspClient;
use tddy_lsp::LspError;

/// Wraps a shared [`LspClient`] with blocking `request` / `notify` entry points.
pub struct LspClientBridge {
    client: Arc<LspClient>,
}

impl LspClientBridge {
    pub fn new(client: Arc<LspClient>) -> Self {
        Self { client }
    }

    /// Send a request and return its `result` field.
    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        tokio::runtime::Handle::current()
            .block_on(self.client.request_raw(method, params))
            .map_err(map_lsp_error)
    }

    /// Send a notification.
    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        tokio::runtime::Handle::current()
            .block_on(self.client.notify_raw(method, params))
            .map_err(map_lsp_error)
    }
}

fn map_lsp_error(error: LspError) -> RestructureError {
    RestructureError::MalformedPlan(format!("lsp: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_a_timeout_into_a_restructure_error() {
        // Given an LSP timeout
        let error = map_lsp_error(LspError::Timeout);

        // Then the message names the failure
        match error {
            RestructureError::MalformedPlan(message) => {
                assert!(message.contains("timed out"));
            }
            other => panic!("expected MalformedPlan, got {other:?}"),
        }
    }
}
