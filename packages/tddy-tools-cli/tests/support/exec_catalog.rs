//! A stand-in for the host side of a session-tool socket that answers the one question
//! `tddy-tools --mcp` asks on every `tools/list`: `ExecToolService/ListExecTools`, "which exec tools
//! is this session offered" (`#project-home` 5/7).
//!
//! `tools/list` asks it over the configured transport and refuses when nothing answers, so a test
//! that wants the exec catalog advertised must put something on the socket that answers. This one
//! answers what a daemon tells an ordinary session — the daemon-wide catalog — and refuses every
//! other call.

// Shared by several test binaries, each of which uses a different subset of these helpers.
#![allow(dead_code)]

use std::path::Path;

use async_trait::async_trait;
use prost::Message;
use tddy_rpc::{RpcMessage, RpcResult, RpcService, Status};
use tddy_service::proto::exec_tools::{ListExecToolsResponse, ToolDef};

/// Whether `service`/`method` is the exec-catalog question.
pub fn is_list_exec_tools(service: &str, method: &str) -> bool {
    service == "exec_tools.ExecToolService" && method == "ListExecTools"
}

/// The answer a daemon gives an ordinary session: every tool of the daemon-wide catalog.
pub fn the_daemon_wide_catalog() -> RpcResult {
    let tools = tddy_tool_engine::tool_catalog()
        .into_iter()
        .map(|tool| ToolDef {
            name: tool.name,
            description: tool.description,
            input_schema_json: tool.input_schema_json,
        })
        .collect();
    RpcResult::Unary(Ok(ListExecToolsResponse { tools }.encode_to_vec()))
}

/// A host that answers `ListExecTools` with [`the_daemon_wide_catalog`] and nothing else.
pub struct HostOfferingTheCatalog;

#[async_trait]
impl RpcService for HostOfferingTheCatalog {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        if is_list_exec_tools(service, method) {
            return the_daemon_wide_catalog();
        }
        RpcResult::Unary(Err(Status::not_found(format!(
            "this stand-in answers only ListExecTools, not {service}/{method}"
        ))))
    }
}

/// Serve [`HostOfferingTheCatalog`] on a Unix socket bound at `socket_path`, one endpoint per
/// connection. Must be called inside a Tokio runtime; the task ends with the runtime.
pub fn a_host_offering_the_catalog_at(socket_path: &Path) -> tokio::task::JoinHandle<()> {
    let _ = std::fs::remove_file(socket_path);
    let listener =
        tokio::net::UnixListener::bind(socket_path).expect("bind the exec-catalog stand-in socket");
    tokio::spawn(async move {
        loop {
            let Ok((stream, _addr)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let (read_half, write_half) = tokio::io::split(stream);
                let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                    read_half,
                    write_half,
                    HostOfferingTheCatalog,
                    tddy_rpc::RequestTransport::UnixSocket,
                );
                endpoint.run().await;
            });
        }
    })
}
