//! Synchronous LSP transport over an existing [`LspClient`].
//!
//! [`RustBackend::from_lsp_client`] stores a long-running client from `tddy-lsp` rather than
//! spawning rust-analyzer itself. The backend's request loop is synchronous; this bridge blocks
//! the current tokio runtime handle on the client's async methods.
//!
//! **Every request is driven by the run's cancellation token**, through
//! [`LspClient::request_abandonable`] rather than `request_raw`. That is the difference between a
//! run that can be interrupted and one that cannot: the backend's poll loops look at the token
//! beside each sleep, but a request *in flight* blocks this thread inside the transport, where no
//! such check runs. A run wedged there used to be unreachable until the client's per-request bound
//! expired — which is why that bound had to be minutes, and why a second `^C` did nothing.
//! Abandoning also sends `$/cancelRequest`, so the server stops computing an answer nobody will
//! read.

use crate::{RestructureError, Result};
use serde_json::Value;
use std::sync::Arc;
use tddy_lsp::client::LspClient;
use tddy_lsp::LspError;
use tokio_util::sync::CancellationToken;

/// Wraps a shared [`LspClient`] with blocking `request` / `notify` entry points.
pub struct LspClientBridge {
    client: Arc<LspClient>,
    /// How a request in flight learns that its caller has stopped waiting.
    cancel: CancellationToken,
}

impl LspClientBridge {
    pub fn new(client: Arc<LspClient>, cancel: CancellationToken) -> Self {
        Self { client, cancel }
    }

    /// Point this bridge at another run's token.
    ///
    /// For a backend whose token arrives after it was built — [`RustBackend::with_cancellation`] is
    /// the case — because a bridge left holding the token it was constructed with would keep every
    /// request beyond the reach of the run that actually owns it.
    pub fn set_cancellation(&mut self, cancel: CancellationToken) {
        self.cancel = cancel;
    }

    /// Send a request and return its `result` field, giving up as soon as the run is cancelled.
    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        tokio::runtime::Handle::current()
            .block_on(
                self.client
                    .request_abandonable(method, params, self.cancel.cancelled()),
            )
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
        // The run's own token ended this request. Deliberately *not* `ServerCatchingUp`: that is
        // the one thing the settle loop retries, and re-asking a server on behalf of a caller who
        // has gone is the opposite of what cancelling meant. It is also not a failure of the
        // server, so it must not read as one.
        LspError::Abandoned => RestructureError::CallerStopped,
        other => RestructureError::MalformedPlan(format!("lsp: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request that outlived its wait is a slow index, not a defective plan. Reporting it as
    /// `MalformedPlan` sends the reader to rewrite anchors that were never wrong, and — because
    /// only `ServerCatchingUp` is retried — skips the retry loop that waits an index out.
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

    /// A cancelled run must not be reported as a defective plan, and must not be *retried*:
    /// `ServerCatchingUp` is the only refusal the settle loop re-asks on, and re-asking a server
    /// for a caller who has gone is the opposite of what cancelling meant.
    #[test]
    fn treats_an_abandoned_request_as_the_caller_stopping_rather_than_a_server_fault() {
        // Given a request the run's own token ended
        let error = map_lsp_error(LspError::Abandoned);

        // Then it names the caller, and is not the one refusal the settle loop retries
        assert!(
            matches!(error, RestructureError::CallerStopped),
            "expected CallerStopped, got {error:?}"
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
