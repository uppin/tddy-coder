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

    /// The server process exited before the request completed.
    #[error("lsp server exited")]
    ServerExited,

    /// An underlying I/O failure.
    #[error("io error: {0}")]
    Io(String),
}
