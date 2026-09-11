//! `tddy-terminal-rpc` — unified PTY-over-RPC bridge shared by `tddy-daemon`, `tddy-coder`, and
//! `tddy-tools`.
//!
//! Owns the [`proto::terminal_session::TerminalSessionService`] proto (the terminal-streaming
//! RPCs previously duplicated between the daemon's gRPC `ConnectionService` and the coder's
//! LiveKit `SessionConnectionServiceRpc`) **and serves it**: [`build_terminal_session_entry`]
//! registers all nine methods over any `tddy-rpc` transport, and
//! [`proto::terminal_session::TerminalSessionServiceTonicAdapter`] serves the same implementation
//! over gRPC / Connect-HTTP. It also owns the transport-agnostic [`TerminalSession`] /
//! [`TerminalSessionStore`] traits, and the streaming bridge functions
//! ([`serve_stream_terminal_output`], [`serve_get_terminal_history`], [`serve_send_terminal_input`])
//! that consolidate the resize / capture-replay / broadcast-subscribe / ACK-framing logic.
//!
//! Replay model: a reconnecting client is shown the current last frame first
//! ([`TerminalCapture::replay_last`]); older bytes are fetched on demand via
//! [`serve_get_terminal_history`] as the user scrolls up, terminating when a chunk arrives with
//! `at_oldest = true`.

pub mod proto {
    /// The canonical message types plus both service-codegen flavors: the tddy-rpc
    /// `TerminalSessionService` trait / `TerminalSessionServiceServer` (LiveKit, stdio) and the
    /// `TerminalSessionServiceTonicAdapter` that delegates one implementation of that trait to the
    /// tonic server trait in [`tonic_terminal_session`].
    pub mod terminal_session {
        include!(concat!(env!("OUT_DIR"), "/terminal_session.rs"));
    }

    /// Tonic-generated gRPC / Connect-HTTP server and client for `terminal_session.proto`, sharing
    /// [`terminal_session`]'s message types via `extern_path`.
    ///
    /// Kept in its own module because tonic-build emits a `TerminalSessionService` trait of its own:
    /// the two flavors share the proto's name and would collide in one namespace. The adapter above
    /// reaches this one through `terminal_session_service_server::TerminalSessionService`.
    pub mod tonic_terminal_session {
        #![allow(unused_imports, clippy::all)]
        include!(concat!(
            env!("OUT_DIR"),
            "/tonic_terminal_session/terminal_session.rs"
        ));
    }
}

pub mod bridge;
pub mod local_pty_relay;
mod local_terminal;
pub mod login_shell;
pub mod pty_relay;
pub mod service;
pub mod session;

pub use bridge::{
    history_into_tonic_stream, into_tonic_stream, serve_get_terminal_history,
    serve_get_terminal_history_with, serve_send_terminal_input,
    serve_stream_session_terminal_io_with, serve_stream_terminal_output,
    serve_stream_terminal_output_with,
};
pub use login_shell::{login_shell_for, login_shell_for_os_user};
pub use proto::terminal_session::{GetTerminalHistoryRequest, TerminalHistoryChunk};
pub use service::{
    build_terminal_session_entry, ControlChange, ControlClaim, TerminalControl, TerminalDescriptor,
    TerminalRoster, TerminalSessionPorts, TerminalSessionServiceImpl,
};
pub use session::{TerminalSession, TerminalSessionStore};
