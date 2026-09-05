//! Webview-IPC flavour of `tddy-rpc`: hosts an RPC service for a webview that reaches it over the
//! host application's IPC bridge rather than over a socket.
//!
//! This is the same shape as [`tddy_stdio`](../../tddy-stdio) — one duplex channel carrying
//! `rpc_envelope` frames — with one difference: an IPC bridge is already message-oriented, so
//! there is no length-prefix frame codec and no chunking. Request frames arrive one per call into
//! [`WebviewRpcHost::handle_request_frame`]; response frames leave through a [`FrameSink`] the
//! webview registers once per page load.
//!
//! The sink is a trait so this crate does not depend on any particular host application: a Tauri
//! app implements it over `tauri::ipc::Channel`, and tests implement it over an in-memory queue.

mod host;

pub use host::{FrameError, FrameSink, SinkClosed, WebviewRpcHost};
