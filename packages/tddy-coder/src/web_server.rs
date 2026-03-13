//! Static file HTTP server for serving the tddy-web bundle.
//!
//! Used when --web-port and --web-bundle-path are both provided.

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::Router;
use tower_http::services::ServeDir;

/// Serve static files from `bundle_path` on the given `port`.
/// Binds to 0.0.0.0 so the server is reachable from other hosts.
pub async fn serve_web_bundle(port: u16, bundle_path: PathBuf) -> anyhow::Result<()> {
    let service = ServeDir::new(&bundle_path).append_index_html_on_directories(true);
    let app = Router::new().fallback_service(service);
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("bind web server port {}: {}", port, e))?;
    log::info!(
        "Web server serving {} on port {}",
        bundle_path.display(),
        port
    );
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("web server error: {}", e))?;
    Ok(())
}
