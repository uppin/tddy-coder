//! Web server and RPC wiring for tddy-daemon.

use std::path::PathBuf;

use tddy_coder::web_server::{serve_web_bundle, ClientConfig};
use tddy_connectrpc::connect_router;
use tddy_rpc::{MultiRpcService, RpcBridge};

/// Start the web server with static bundle and RPC services.
pub async fn run_server(
    host: &str,
    port: u16,
    bundle_path: PathBuf,
    rpc_entries: Vec<tddy_rpc::ServiceEntry>,
    livekit_url: Option<String>,
) -> anyhow::Result<()> {
    let rpc_router = if rpc_entries.is_empty() {
        None
    } else {
        let multi = MultiRpcService::new(rpc_entries);
        Some(connect_router(RpcBridge::new(multi)))
    };

    let client_config = ClientConfig {
        livekit_url,
        livekit_room: None,
        daemon_mode: Some(true),
    };
    serve_web_bundle(host, port, bundle_path, rpc_router, Some(client_config)).await
}
