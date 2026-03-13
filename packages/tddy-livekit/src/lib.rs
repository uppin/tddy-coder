//! tddy-livekit: LiveKit RPC transport for tddy-coder.
//!
//! Custom protobuf-based RPC over LiveKit data channels.

pub mod bridge;
pub mod client;
pub mod echo_service;
pub mod envelope;
pub mod participant;
pub mod rpc_log;
pub mod status;
pub mod terminal_service;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/rpc.rs"));
    pub mod test {
        include!(concat!(env!("OUT_DIR"), "/test.rs"));
    }
    pub mod terminal {
        include!(concat!(env!("OUT_DIR"), "/terminal.rs"));
    }
}

pub use bridge::{RpcBridge, RpcResult, RpcService};
pub use client::{BidiStreamSender, RpcClient};
pub use echo_service::{create_echo_bridge, EchoServiceImpl};
pub use envelope::{decode_request, encode_request, encode_response, response_from_result};
pub use livekit::prelude::RoomOptions;
pub use participant::LiveKitParticipant;
pub use status::Status;
pub use terminal_service::TerminalServiceImpl;
