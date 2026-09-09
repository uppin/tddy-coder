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

    /// Take every server notification received since the last drain, oldest first.
    pub fn drain_notifications(&self) -> Vec<Value> {
        self.client.drain_notifications()
    }

    /// The `initialize` result the shared client negotiated with the server.
    pub fn handshake(&self) -> Value {
        self.client.handshake()
    }

    /// Send a notification.
    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        tokio::runtime::Handle::current()
            .block_on(self.client.notify_raw(method, params))
            .map_err(map_lsp_error)
    }
}

/// LSP `ContentModified` — rust-analyzer's "the document changed under this request, ask again".
const CONTENT_MODIFIED: i64 = -32801;

/// Classify a transport failure by whether waiting can fix it.
///
/// Only [`RestructureError::ServerCatchingUp`] is retried by the backend's assist and settle
/// loops, so anything retryable that lands anywhere else skips those loops entirely — which is
/// how a slow index came to be reported as a malformed plan.
fn map_lsp_error(error: LspError) -> RestructureError {
    match error {
        // The server asking to be asked again. The self-spawned transport has always mapped
        // this; the bridge dropped it, leaving the settle loop dead on this path.
        LspError::Server {
            code: CONTENT_MODIFIED,
            ..
        } => RestructureError::ServerCatchingUp,
        // A request that outlived its wait. The plan is not at fault and the anchors are not
        // wrong; the index was not ready, which is precisely what the retry loops wait out.
        LspError::Timeout => RestructureError::ServerCatchingUp,
        other => RestructureError::MalformedPlan(format!("lsp: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request that outlived its wait is a slow index, not a defective plan. Reporting it as
    /// `MalformedPlan` sends the reader to rewrite anchors that were never wrong, and — because
    /// only `ServerCatchingUp` is retried — skips the very loop `--indexing-budget` governs.
    #[test]
    fn treats_a_timeout_as_a_server_still_catching_up() {
        // Given a request that timed out
        let error = map_lsp_error(LspError::Timeout);

        // Then it is retryable rather than a plan defect
        assert!(
            matches!(error, RestructureError::ServerCatchingUp),
            "expected ServerCatchingUp, got {error:?}"
        );
    }

    /// `ContentModified` is rust-analyzer asking to be asked again. The self-spawned transport
    /// has always mapped it; the bridge dropped it, which left `request_settled`'s retry loop
    /// dead on this path.
    #[test]
    fn treats_content_modified_as_a_server_still_catching_up() {
        // Given the server answering with `ContentModified`
        let error = map_lsp_error(LspError::Server {
            code: -32801,
            message: "content modified".to_string(),
        });

        // Then it is retryable rather than a plan defect
        assert!(
            matches!(error, RestructureError::ServerCatchingUp),
            "expected ServerCatchingUp, got {error:?}"
        );
    }

    /// Every other server error is a real failure and must keep naming itself, rather than
    /// being retried until the budget runs out and reported as an absent assist.
    #[test]
    fn reports_any_other_server_error_verbatim() {
        // Given the server answering with an internal error
        let error = map_lsp_error(LspError::Server {
            code: -32603,
            message: "internal error".to_string(),
        });

        // Then the code and message survive into the reported failure
        match error {
            RestructureError::MalformedPlan(message) => {
                assert!(message.contains("-32603"), "{message}");
                assert!(message.contains("internal error"), "{message}");
            }
            other => panic!("expected MalformedPlan, got {other:?}"),
        }
    }
}
