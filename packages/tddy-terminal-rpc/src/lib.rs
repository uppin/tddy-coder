//! `tddy-terminal-rpc` — unified PTY-over-RPC bridge shared by `tddy-daemon`, `tddy-coder`, and
//! `tddy-tools`.
//!
//! Owns the [`proto::terminal_session::TerminalSessionService`] proto (the terminal-streaming
//! RPCs previously duplicated between the daemon's gRPC `ConnectionService` and the coder's
//! LiveKit `SessionConnectionServiceRpc`), the transport-agnostic [`TerminalSession`] /
//! [`TerminalSessionStore`] traits, and the streaming bridge functions
//! ([`serve_stream_terminal_output`], [`serve_get_terminal_history`], [`serve_send_terminal_input`])
//! that consolidate the resize / capture-replay / broadcast-subscribe / ACK-framing logic.
//!
//! Replay model: a reconnecting client is shown the current last frame first
//! ([`TerminalCapture::replay_last`]); older bytes are fetched on demand via
//! [`serve_get_terminal_history`] as the user scrolls up, terminating when a chunk arrives with
//! `at_oldest = true`.

pub mod proto {
    pub mod terminal_session {
        include!(concat!(env!("OUT_DIR"), "/terminal_session.rs"));
    }
}

pub mod bridge;
pub mod local_pty_relay;
pub mod session;

pub use bridge::{
    history_into_tonic_stream, into_tonic_stream, serve_get_terminal_history,
    serve_get_terminal_history_with, serve_send_terminal_input,
    serve_stream_session_terminal_io_with, serve_stream_terminal_output,
    serve_stream_terminal_output_with,
};
pub use proto::terminal_session::{GetTerminalHistoryRequest, TerminalHistoryChunk};
pub use session::{TerminalSession, TerminalSessionStore};

/// Attaching a terminal from outside the daemon, over whichever transport is available.
///
/// Moved here from `tddy-tools` by `#unbundle` node 5. This crate's own description already named
/// `tddy-tools` as a consumer and it already owned [`local_pty_relay`], so the client half belonged
/// here from the start — `run_local_pty` was already delegating into this crate.
///
/// **Node 6 serves `terminal_session.TerminalSessionService` from this crate**, on top of the bridge
/// it already owns. Its shape is fixed here so node 6 compiles against it.
pub mod pty_relay {
    /// How a relay reaches the terminal it attaches to.
    ///
    /// Four modes rather than two because "connect to a session that already exists" and "start one
    /// and then connect" fail differently: the first on a session that is gone, the second on a
    /// session that could not be created. A caller shown one message for both cannot tell which.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RelayMode {
        /// A PTY in this process, delegated to [`super::local_pty_relay`].
        LocalPty,
        /// gRPC to an existing session's terminal.
        GrpcConnect { url: String, session_id: String },
        /// gRPC, starting the session first.
        GrpcStartAndConnect { url: String },
        /// A LiveKit room carrying the terminal.
        LiveKitSession { room: String },
    }

    /// Why a relay could not attach.
    #[derive(Debug, thiserror::Error)]
    pub enum RelayError {
        #[error("session {session_id} does not exist")]
        NoSuchSession { session_id: String },
        #[error("the session could not be started: {reason}")]
        StartFailed { reason: String },
    }

    /// Attach a terminal in the given mode, returning when the far side closes it.
    pub async fn run_pty_relay(_mode: RelayMode) -> Result<(), RelayError> {
        // TODO(tools-thinning): implement
        unimplemented!("pty_relay::run_pty_relay")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A gone session and an unstartable one are different problems for the operator, so they
        /// must not render the same.
        #[test]
        fn tells_a_missing_session_apart_from_one_that_would_not_start() {
            let missing = RelayError::NoSuchSession {
                session_id: "session-a".to_string(),
            };
            let unstartable = RelayError::StartFailed {
                reason: "no worktree".to_string(),
            };

            assert!(missing.to_string().contains("does not exist"));
            assert!(unstartable.to_string().contains("could not be started"));
        }

        #[tokio::test]
        async fn refuses_to_connect_to_a_session_that_does_not_exist() {
            // Given
            let mode = RelayMode::GrpcConnect {
                url: "http://127.0.0.1:0".to_string(),
                session_id: "session-that-is-gone".to_string(),
            };

            // When
            let outcome = run_pty_relay(mode).await;

            // Then
            assert!(matches!(outcome, Err(RelayError::NoSuchSession { .. })));
        }
    }
}
