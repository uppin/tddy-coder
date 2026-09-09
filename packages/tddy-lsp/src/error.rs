//! Error type for the LSP subsystem.

/// Failures raised while selecting, spawning, or talking to a language server.
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    /// The requested language is not present in the allow-list — no server is spawned.
    #[error("language not allowed: {0}")]
    LanguageNotAllowed(String),

    /// The configured server program could not be found (e.g. not on PATH).
    #[error("language server not found: {0}")]
    ServerNotFound(String),

    /// The server sent a malformed or unexpected JSON-RPC message.
    #[error("lsp protocol error: {0}")]
    Protocol(String),

    /// A request did not receive a response within the timeout.
    #[error("lsp request timed out")]
    Timeout,

    /// The server answered the request with a JSON-RPC `error` instead of a `result`.
    ///
    /// The code is the server's own, and callers dispatch on it: rust-analyzer's
    /// `ContentModified` (-32801) means "ask again" rather than "this failed", and a caller
    /// that cannot tell the two apart either retries forever or gives up on a live server.
    #[error("lsp server error {code}: {message}")]
    Server { code: i64, message: String },

    /// The server process exited before the request completed.
    #[error("lsp server exited")]
    ServerExited,

    /// An underlying I/O failure.
    #[error("io error: {0}")]
    Io(String),
}
