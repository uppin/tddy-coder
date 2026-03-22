//! tddy-livekit: LiveKit RPC transport for tddy-coder.
//!
//! Custom protobuf-based RPC over LiveKit data channels.
//! Thin transport adapter — delegates to tddy-rpc for generic dispatch.

pub mod bridge;
pub mod client;
pub mod envelope;
pub mod participant;
pub mod rpc_log;
pub mod runtime_status_bridge;
pub mod token;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/rpc.rs"));
}

pub use bridge::{RpcBridge, RpcResult, RpcService};
pub use client::{BidiStreamSender, RpcClient};
pub use envelope::{decode_request, encode_request, encode_response, response_from_result};
pub use livekit::prelude::RoomOptions;
pub use participant::LiveKitParticipant;
pub use runtime_status_bridge::session_runtime_status_envelope_should_forward;
pub use tddy_rpc::Status;
pub use token::TokenGenerator;
