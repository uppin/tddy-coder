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
