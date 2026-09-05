//! Webview-IPC flavour of `tddy-rpc`: hosts an RPC service for a webview that reaches it over the
//! host application's IPC bridge rather than over a socket.
//!
//! This is the same shape as [`tddy_stdio`](../../tddy-stdio) — one duplex channel carrying
//! `rpc_envelope` frames — with one difference: an IPC bridge is already message-oriented, so
//! there is no length-prefix frame codec and no chunking. Request frames arrive one per call into
//! the host's `handle_request_frame` — [`MultiConnectionHost::handle_request_frame`] is the one the
//! desktop runs — and are routed by the client epoch they carry; response frames leave through a
//! [`FrameSink`] the webview registers once per connection, of which a page may hold several.
//!
//! The sink is a trait so this crate does not depend on any particular host application: a Tauri
//! app implements it over `tauri::ipc::Channel`, and tests implement it over an in-memory queue.
//!
//! ## One connection, or many
//!
//! [`WebviewRpcHost`] serves a single webview connection reaching a single service — the shape every
//! page had while there was only one thing to reach. [`MultiConnectionHost`] serves **many
//! concurrent, independently addressed** connections, each resolving its [`ConnectionTarget`] to a
//! roster through a [`RosterResolver`]. That is what lets one page hold a connection to the daemon
//! and one per attached session at the same time — the IPC equivalent of a LiveKit room plus a
//! participant per session, with nothing LiveKit-shaped in it.

mod host;
mod multi_host;
mod target;

pub use host::{FrameError, FrameSink, SinkClosed, WebviewRpcHost};
pub use multi_host::MultiConnectionHost;
pub use target::{ConnectError, ConnectionTarget, RosterResolver};
