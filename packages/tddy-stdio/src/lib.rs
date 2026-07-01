//! stdio/IPC transport for `tddy-rpc` — parent/child process RPC over stdin/stdout.
//!
//! Layers [`tddy_rpc::client_engine::ClientEngine`] and [`tddy_rpc::server_engine::ServerEngine`]
//! over a length-prefixed framed byte channel (see [`tddy_rpc::transport`]). A single duplex pipe
//! pair carries both `Request` and `Response` frames, discriminated by [`tddy_rpc::transport::FrameKind`]
//! — which is what lets one peer be both an RPC client and server over the same channel: either
//! side can call into the other.

mod client;
mod endpoint;

pub use client::{StdioBidiSender, StdioRpcClient};
pub use endpoint::{spawn_child_endpoint, ChildEndpoint, StdioEndpoint};
