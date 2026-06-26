//! Static file HTTP server for serving the tddy-web bundle.
//!
//! Used when --web-port and --web-bundle-path are both provided.

use std::path::PathBuf;

use axum::routing::get;
use axum::{Json, Router};
use tower_http::services::{ServeDir, ServeFile};

/// One backend row for [`ClientConfig::allowed_agents`] (daemon `allowed_agents` YAML).
#[derive(Clone, serde::Serialize)]
pub struct ClientAllowedAgent {
    pub id: String,
    pub label: String,
}

/// Client-visible server config, served at /api/config.
#[derive(Clone, serde::Serialize)]
pub struct ClientConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livekit_room: Option<String>,
    /// Shared presence room (daemon `livekit.common_room`). Browser joins for participant list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_room: Option<String>,
    /// When true, server is tddy-daemon; show ConnectionScreen instead of ConnectionForm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_mode: Option<bool>,
    /// Daemon: same allowlist as `ListAgents` / `allowed_agents` in YAML (for UI before RPC hydrates).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_agents: Vec<ClientAllowedAgent>,
    /// Browser DEBUG mask (`debug`-package namespaces, e.g. `tddy:term:*`). From daemon `debug:` YAML;
    /// the web app enables scoped `[tddy]` diagnostics from this. None = off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<String>,
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
    serve_web_bundle_with_shutdown(
        host,
        port,
        bundle_path,
        rpc_router,
        client_config,
        std::future::pending(),
    )
    .await
}

/// Same as [`serve_web_bundle`], but stops the HTTP server when `shutdown` completes (graceful shutdown).
pub async fn serve_web_bundle_with_shutdown<F>(
    host: impl AsRef<str>,
    port: u16,
    bundle_path: PathBuf,
    rpc_router: Option<Router>,
    client_config: Option<ClientConfig>,
    shutdown: F,
) -> anyhow::Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
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
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|e| anyhow::anyhow!("web server error: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_config() -> ClientConfig {
        ClientConfig {
            livekit_url: None,
            livekit_room: None,
            common_room: None,
            daemon_mode: None,
            allowed_agents: vec![],
            debug: None,
        }
    }

    #[test]
    fn debug_mask_is_omitted_from_api_config_when_none() {
        let json = serde_json::to_value(empty_config()).expect("serialize");
        assert!(
            json.get("debug").is_none(),
            "debug must be skipped when None: {json}"
        );
    }

    #[test]
    fn debug_mask_is_serialized_when_set() {
        let cfg = ClientConfig {
            debug: Some("tddy:term:*".to_string()),
            ..empty_config()
        };
        let json = serde_json::to_value(cfg).expect("serialize");
        assert_eq!(
            json.get("debug").and_then(|v| v.as_str()),
            Some("tddy:term:*")
        );
    }
}
