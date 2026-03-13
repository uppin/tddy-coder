//! Re-export tddy-rpc bridge types.
//!
//! The participant uses tddy_rpc::RpcBridge and converts RpcRequest to RpcMessage before dispatching.

pub use tddy_rpc::{ResponseBody, RpcBridge, RpcResult, RpcService};
