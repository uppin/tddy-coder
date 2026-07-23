//! tddy-livekit: LiveKit RPC transport for tddy-coder.
//!
//! Custom protobuf-based RPC over LiveKit data channels.
//! Thin transport adapter — delegates to tddy-rpc for generic dispatch.

pub mod bridge;
pub mod chunking;
pub mod client;
pub mod client_factory;
pub mod envelope;
pub mod participant;
mod projects_registry;
pub mod rpc_log;
#[cfg(test)]
pub mod test_util;
pub mod token;

/// The RPC envelope (`RpcRequest`/`RpcResponse`/...) is compiled once in `tddy-rpc` and shared by
/// every transport — re-exported here so existing `crate::proto::*` paths keep resolving.
pub mod proto {
    pub use tddy_rpc::envelope::*;
}

pub use bridge::{RpcBridge, RpcResult, RpcService};
pub use client::{BidiStreamSender, RpcClient};
pub use client_factory::LiveKitRpcClientFactory;
pub use envelope::{decode_request, encode_request, encode_response, response_from_result};
pub use livekit::prelude::RoomOptions;
pub use participant::{
    merge_participant_metadata_json, owned_project_count_for_projects_dir,
    spawn_local_participant_metadata_watcher, LiveKitParticipant, OWNED_PROJECT_COUNT_METADATA_KEY,
};
pub use tddy_rpc::Status;
pub use token::{TokenGenerator, DEFAULT_LIVEKIT_JWT_TTL_SECS};
