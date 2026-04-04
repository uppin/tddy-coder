//! Web server and RPC wiring for tddy-daemon.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_coder::web_server::{serve_web_bundle_with_shutdown, ClientConfig};
use tddy_connectrpc::connect_router;
use tddy_rpc::{MultiRpcService, RpcBridge};

use crate::config::DaemonConfig;
use crate::telegram_notifier::{send_daemon_lifecycle_message, TelegramSender};

/// Start the web server with static bundle and RPC services.
pub async fn run_server(
    host: &str,
    port: u16,
    bundle_path: PathBuf,
    rpc_entries: Vec<tddy_rpc::ServiceEntry>,
    livekit_url: Option<String>,
    common_room: Option<String>,
    lifecycle_telegram: Option<(DaemonConfig, Arc<dyn TelegramSender + Send + Sync>)>,
) -> anyhow::Result<()> {
    if let Some((ref cfg, ref sender)) = lifecycle_telegram {
        send_daemon_lifecycle_message(cfg, sender.as_ref(), "tddy-daemon started").await?;
    }

    let rpc_router = if rpc_entries.is_empty() {
        None
    } else {
        let multi = MultiRpcService::new(rpc_entries);
        Some(connect_router(RpcBridge::new(multi)))
    };

    let client_config = ClientConfig {
        livekit_url,
        livekit_room: None,
        common_room,
        daemon_mode: Some(true),
    };

    let shutdown_copy = lifecycle_telegram.clone();
    let shutdown = async move {
        shutdown_signal().await;
        if let Some((cfg, sender)) = shutdown_copy {
            let _ =
                send_daemon_lifecycle_message(&cfg, sender.as_ref(), "tddy-daemon stopped").await;
        }
    };

    serve_web_bundle_with_shutdown(
        host,
        port,
        bundle_path,
        rpc_router,
        Some(client_config),
        shutdown,
    )
    .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
