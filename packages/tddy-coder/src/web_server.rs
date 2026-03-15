//! Static file HTTP server for serving the tddy-web bundle.
//!
//! Used when --web-port and --web-bundle-path are both provided.

use std::path::PathBuf;

use axum::routing::get;
use axum::{Json, Router};
use tower_http::services::{ServeDir, ServeFile};

/// Client-visible server config, served at /api/config.
#[derive(Clone, serde::Serialize)]
pub struct ClientConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_room: Option<String>,
}

/// Serve static files from `bundle_path` on the given `host` and `port`.
/// When `rpc_router` is provided, it is merged before the static file fallback (e.g. ConnectRPC at /rpc).
/// When `client_config` is provided, it is served at GET /api/config.
/// Unmatched routes fall back to index.html for SPA client-side routing.
pub async fn serve_web_bundle(
    host: impl AsRef<str>,
    port: u16,
    bundle_path: PathBuf,
    rpc_router: Option<Router>,
    client_config: Option<ClientConfig>,
) -> anyhow::Result<()> {
    let host = host.as_ref();
    let index_path = bundle_path.join("index.html");
    let service = ServeDir::new(&bundle_path)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(&index_path));
    let mut app = Router::new();
    if let Some(config) = client_config {
        app = app.route(
            "/api/config",
            get(move || {
                let config = config.clone();
                async move { Json(config) }
            }),
        );
    }
    if let Some(rpc) = rpc_router {
        app = app.merge(rpc);
    }
    app = app.fallback_service(service);
    let addr = (host, port);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("bind web server {}:{}: {}", host, port, e))?;
    log::info!(
        "Web server serving {} on {}:{}",
        bundle_path.display(),
        host,
        port
    );
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("web server error: {}", e))?;
    Ok(())
}
