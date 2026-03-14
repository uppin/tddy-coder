//! Connect protocol HTTP transport for tddy-rpc services.
//!
//! Exposes RpcService implementations over HTTP using the Connect protocol.

pub mod envelope;
mod error;
mod protocol;
mod router;

pub use router::connect_router;
