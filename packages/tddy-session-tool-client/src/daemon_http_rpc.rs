//! A `tddy-rpc` client over the daemon's HTTP `/rpc` endpoint (the Connect protocol, protobuf
//! payloads) — what lets `tddy-tools` in remote-codebase mode reach the roster and conversation
//! RPCs through the local relay daemon, which forwards them to the codebase daemon named by each
//! request's `daemon_instance_id`.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-remote-codebase.md

use async_trait::async_trait;
use tddy_rpc::{RpcClientTransport, Status};
use tokio::sync::mpsc;

/// Connect-protocol client for `POST <base_url>/<service>/<method>`: unary calls as
/// `application/proto`, server streams as enveloped `application/connect+proto`.
#[derive(Debug, Clone)]
pub struct DaemonHttpRpcClient {
    /// The daemon's RPC base, e.g. `http://127.0.0.1:8899/rpc`.
    pub base_url: String,
}

impl DaemonHttpRpcClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }
}

fn not_implemented(what: &str) -> Status {
    Status::unimplemented(format!(
        "TODO(remote-codebase): DaemonHttpRpcClient::{what} is not implemented"
    ))
}

#[async_trait]
impl RpcClientTransport for DaemonHttpRpcClient {
    async fn call_unary(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        // TODO(remote-codebase): implement
        Err(not_implemented("call_unary"))
    }

    async fn call_server_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        // TODO(remote-codebase): implement
        Err(not_implemented("call_server_stream"))
    }

    async fn call_client_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::unimplemented(
            "the daemon's HTTP endpoint serves no client-streaming RPC this client is used for",
        ))
    }

    async fn call_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented(
            "the daemon's HTTP endpoint serves no bidirectional RPC this client is used for",
        ))
    }
}
