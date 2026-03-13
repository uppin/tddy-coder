//! Generic RPC framework — transport-agnostic types and dispatch.

pub mod bridge;
pub mod message;
pub mod status;
pub mod types;

pub use bridge::{ResponseBody, RpcBridge, RpcResult, RpcService};
pub use message::{RequestMetadata, RpcMessage};
pub use status::{Code, Status};
pub use types::{Request, Response, Streaming};
