//! Serve the daemon's `ConnectionService` over a local Unix-domain socket with tonic gRPC.
//!
//! The local socket is the peer-trust transport: tonic populates each request's `UdsConnectInfo`
//! with the caller's SO_PEERCRED credentials, which the [`ConnectionServiceTonicAdapter`] reads in
//! `MintLocalToken`. This is spawned as an independent task alongside the HTTP server; it shares
//! the same `ConnectionServiceImpl` instance (via `Arc`) so sessions started over the socket are
//! visible over every other transport.

use std::future::Future;
use std::path::Path;

use anyhow::Context;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;

use tddy_service::proto::connection::ConnectionService as RpcConnectionService;
use tddy_service::tonic_connection::connection_service_server::ConnectionServiceServer;

use crate::connection_tonic_adapter::ConnectionServiceTonicAdapter;

/// Bind `socket_path` and serve `adapter`'s `ConnectionService` until `shutdown` resolves.
///
/// A stale socket left by a previous run is unlinked first so the bind does not fail with
/// `EADDRINUSE`. The parent directory is created if missing.
pub async fn serve_connection_uds<T>(
    socket_path: &Path,
    adapter: ConnectionServiceTonicAdapter<T>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()>
where
    T: RpcConnectionService,
    T::StreamSessionTerminalIoStream: 'static,
    T::StreamTerminalOutputStream: 'static,
    T::WatchTerminalControlStream: 'static,
{
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create local socket dir {}", parent.display()))?;
    }
    let _ = std::fs::remove_file(socket_path);
    let listener = tokio::net::UnixListener::bind(socket_path)
        .with_context(|| format!("bind local socket {}", socket_path.display()))?;
    log::info!(
        target: "tddy_daemon::local_socket_server",
        "ConnectionService listening on local socket {}",
        socket_path.display()
    );
    Server::builder()
        .add_service(ConnectionServiceServer::new(adapter))
        .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown)
        .await
        .context("serve ConnectionService over local socket")?;
    Ok(())
}
