//! Web server and RPC wiring for tddy-daemon.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_coder::web_server::{serve_web_bundle_with_shutdown, ClientAllowedAgent, ClientConfig};
use tddy_connectrpc::connect_router;
use tddy_rpc::{MultiRpcService, RpcBridge};

use crate::config::DaemonConfig;
use tddy_session_lifecycle::livekit_peer_discovery::local_instance_id_for_config;
use tddy_session_lifecycle::telegram_notifier::{send_daemon_lifecycle_message, TelegramSender};

/// Everything [`run_server`] needs to bring the daemon's HTTP surface up.
///
/// One struct rather than a positional list because the fields are wiring a caller assembles
/// piecemeal — every one of them reads as a name at the call site, and adding or removing a
/// service is a field, not a renumbering.
pub struct RunServerOptions {
    /// Interface the HTTP listener binds to (daemon `listen.web_host`).
    pub host: String,
    /// Port the HTTP listener binds to (daemon `listen.web_port`).
    pub port: u16,
    /// Directory holding the built `tddy-web` bundle, served as static files with an
    /// `index.html` fallback for client-side routes. Empty in relay mode, which serves no page.
    pub bundle_path: PathBuf,
    /// Services published over ConnectRPC at `/rpc/{service}/{method}`. No entries, no RPC route.
    pub rpc_entries: Vec<tddy_rpc::ServiceEntry>,
    /// LiveKit URL the served page connects to (`livekit.public_url`, else `livekit.url`).
    pub livekit_url: Option<String>,
    /// Shared presence room the served page joins (`livekit.common_room`).
    pub common_room: Option<String>,
    /// Whether this daemon joins the common room above (`livekit.enabled`). The page is told, so a
    /// daemon that stays out of it serves a page that does too.
    pub livekit_enabled: bool,
    /// This daemon's own instance id, so the page can tell which common-room daemon served it.
    pub daemon_instance_id: String,
    /// Startup snapshot of the agent allowlist, for the UI before `ListAgents` hydrates it.
    pub allowed_agents: Vec<ClientAllowedAgent>,
    /// Browser `DEBUG` mask served at `/api/config` (daemon `debug`). `None` = off.
    pub debug: Option<String>,
    /// Config and sender for the "started"/"stopped" Telegram lifecycle messages. `None` sends none.
    pub lifecycle_telegram: Option<(DaemonConfig, Arc<dyn TelegramSender + Send + Sync>)>,
    /// Lets an external task (e.g. an idle-timeout monitor) trigger graceful shutdown without
    /// ctrl_c or SIGTERM. When `None`, only OS signals shut the server down.
    pub shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>,
}

/// Start the web server with static bundle and RPC services.
pub async fn run_server(options: RunServerOptions) -> anyhow::Result<()> {
    let RunServerOptions {
        host,
        port,
        bundle_path,
        rpc_entries,
        livekit_url,
        common_room,
        livekit_enabled,
        daemon_instance_id,
        allowed_agents,
        debug,
        lifecycle_telegram,
        shutdown_rx,
    } = options;

    if let Some((ref cfg, ref sender)) = lifecycle_telegram {
        let instance_id = local_instance_id_for_config(cfg);
        let msg = format!("tddy-daemon started ({})", instance_id);
        send_daemon_lifecycle_message(cfg, sender.as_ref(), &msg).await?;
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
        allowed_agents,
        debug,
        daemon_instance_id: Some(daemon_instance_id),
        livekit_enabled: Some(livekit_enabled),
    };

    let shutdown_copy = lifecycle_telegram.clone();
    let shutdown = async move {
        // Either an OS signal or an external channel fires the shutdown.
        if let Some(rx) = shutdown_rx {
            tokio::select! {
                _ = shutdown_signal() => {}
                _ = async { let _ = rx.await; } => {}
            }
        } else {
            shutdown_signal().await;
        }
        if let Some((cfg, sender)) = shutdown_copy {
            let _ =
                send_daemon_lifecycle_message(&cfg, sender.as_ref(), "tddy-daemon stopped").await;
        }
    };

    serve_web_bundle_with_shutdown(
        host.as_str(),
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
