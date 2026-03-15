//! Generic RPC framework — transport-agnostic types and dispatch.

pub mod bridge;
pub mod message;
pub mod status;
pub mod types;

pub use bridge::{
    BidiStreamOutput, MultiRpcService, ResponseBody, RpcBridge, RpcResult, RpcService, ServiceEntry,
};
pub use message::{RequestMetadata, RpcMessage};
pub use status::{Code, Status};
pub use types::{Request, Response, Streaming};
