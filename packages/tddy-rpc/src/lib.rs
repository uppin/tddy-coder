//! Generic RPC framework — transport-agnostic types and dispatch.

pub mod bridge;
pub mod client_engine;
pub mod envelope;
pub mod message;
pub mod server_engine;
pub mod status;
pub mod transport;
pub mod types;

pub use bridge::{
    BidiStreamOutput, MultiRpcService, ResponseBody, RpcBridge, RpcResult, RpcService, ServiceEntry,
};
pub use message::{RequestMetadata, RpcMessage};
pub use status::{Code, Status};
pub use transport::RpcClientTransport;
pub use types::{Request, Response, Streaming};
