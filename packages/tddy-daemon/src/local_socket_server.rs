//! Serve the daemon's `ConnectionService` over a local Unix-domain socket with tonic gRPC.
//!
//! The local socket is the peer-trust transport: tonic populates each request's `UdsConnectInfo`
//! with the caller's SO_PEERCRED credentials, which the [`ConnectionServiceTonicAdapter`] reads in
//! `MintLocalToken`. This is spawned as an independent task alongside the HTTP server; it shares
//! the same `ConnectionServiceImpl` instance (via `Arc`) so sessions started over the socket are
//! visible over every other transport.

use std::future::Future;
use std::os::unix::io::{FromRawFd, RawFd};
use std::path::{Path, PathBuf};

use anyhow::Context;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;

use tddy_service::proto::connection::ConnectionService as RpcConnectionService;
use tddy_service::tonic_connection::connection_service_server::ConnectionServiceServer;

use crate::connection_tonic_adapter::ConnectionServiceTonicAdapter;

/// First file descriptor systemd passes for socket activation (see `sd_listen_fds(3)`).
pub const SD_LISTEN_FDS_START: RawFd = 3;

/// Where the listening socket comes from.
#[derive(Debug, PartialEq)]
pub enum SocketSource {
    /// A listener inherited from systemd socket activation, at the given file descriptor.
    Activated(RawFd),
    /// No usable activation environment; bind the given path ourselves.
    SelfBind(PathBuf),
}

/// Decide whether to adopt a systemd-passed activation fd or bind the socket path ourselves.
///
/// Systemd sets `LISTEN_PID` to the pid it expects to consume the fds and `LISTEN_FDS` to the
/// number of fds passed (starting at [`SD_LISTEN_FDS_START`]). We only adopt the activation fd
/// when `LISTEN_PID` names this process and at least one fd was passed. Any missing, mismatched,
/// or malformed value falls back to self-binding `fallback_path`.
pub fn resolve_socket_source(
    my_pid: u32,
    listen_pid: Option<&str>,
    listen_fds: Option<&str>,
    fallback_path: &Path,
) -> SocketSource {
    let self_bind = || SocketSource::SelfBind(fallback_path.to_path_buf());

    let Some(pid) = listen_pid.and_then(|v| v.parse::<u32>().ok()) else {
        return self_bind();
    };
    if pid != my_pid {
        return self_bind();
    }
    let Some(fds) = listen_fds.and_then(|v| v.parse::<i32>().ok()) else {
        return self_bind();
    };
    if fds < 1 {
        return self_bind();
    }
    SocketSource::Activated(SD_LISTEN_FDS_START)
}

/// Bind `socket_path` and serve `adapter`'s `ConnectionService` until `shutdown` resolves.
///
/// When launched via systemd socket activation (`LISTEN_PID`/`LISTEN_FDS` addressed to this
/// process), the inherited listener is adopted instead — systemd owns the socket node and its
/// permissions, so no directory is created, no stale file is unlinked, and no chmod is applied.
/// Otherwise a stale socket left by a previous run is unlinked first so the bind does not fail
/// with `EADDRINUSE`, and the parent directory is created if missing.
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
    let listen_pid = std::env::var("LISTEN_PID").ok();
    let listen_fds = std::env::var("LISTEN_FDS").ok();
    let source = resolve_socket_source(
        std::process::id(),
        listen_pid.as_deref(),
        listen_fds.as_deref(),
        socket_path,
    );

    let listener = match source {
        SocketSource::Activated(fd) => {
            // Consume the activation environment so we do not leak it to child processes.
            std::env::remove_var("LISTEN_PID");
            std::env::remove_var("LISTEN_FDS");
            std::env::remove_var("LISTEN_FDNAMES");

            // SAFETY: systemd guarantees fd `SD_LISTEN_FDS_START` is an open, listening
            // AF_UNIX socket when LISTEN_PID matches our pid and LISTEN_FDS >= 1. We take
            // sole ownership of it here and never touch the raw fd again.
            let std_listener = unsafe { std::os::unix::net::UnixListener::from_raw_fd(fd) };
            std_listener
                .set_nonblocking(true)
                .context("set adopted activation socket non-blocking")?;
            let listener = tokio::net::UnixListener::from_std(std_listener)
                .context("adopt systemd activation socket")?;
            log::info!(
                target: "tddy_daemon::local_socket_server",
                "ConnectionService adopted systemd activation fd {fd} (socket label {})",
                socket_path.display()
            );
            listener
        }
        SocketSource::SelfBind(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create local socket dir {}", parent.display()))?;
            }
            let _ = std::fs::remove_file(&path);
            let listener = tokio::net::UnixListener::bind(&path)
                .with_context(|| format!("bind local socket {}", path.display()))?;
            log::info!(
                target: "tddy_daemon::local_socket_server",
                "ConnectionService listening on local socket {}",
                path.display()
            );
            listener
        }
    };

    Server::builder()
        .add_service(ConnectionServiceServer::new(adapter))
        .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown)
        .await
        .context("serve ConnectionService over local socket")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{resolve_socket_source, SocketSource, SD_LISTEN_FDS_START};
    use std::path::PathBuf;

    fn a_socket_path() -> PathBuf {
        PathBuf::from("/run/tddy-daemon.sock")
    }

    #[test]
    fn adopts_the_systemd_activation_fd_when_it_is_addressed_to_this_process() {
        // Given systemd launched us with exactly one activation fd, tagged with our pid
        let my_pid = 4242;

        // When we resolve where the listening socket comes from
        let source = resolve_socket_source(my_pid, Some("4242"), Some("1"), &a_socket_path());

        // Then we adopt the first passed fd instead of binding the path ourselves
        assert_eq!(source, SocketSource::Activated(SD_LISTEN_FDS_START));
    }

    #[test]
    fn self_binds_when_no_activation_environment_is_present() {
        // Given the daemon was run directly, with no LISTEN_PID / LISTEN_FDS
        let my_pid = 4242;

        // When
        let source = resolve_socket_source(my_pid, None, None, &a_socket_path());

        // Then we fall back to binding the configured path ourselves
        assert_eq!(source, SocketSource::SelfBind(a_socket_path()));
    }

    #[test]
    fn self_binds_when_the_activation_fds_are_addressed_to_another_process() {
        // Given LISTEN_PID names a different process (fds were not meant for us)
        let my_pid = 4242;

        // When
        let source = resolve_socket_source(my_pid, Some("9999"), Some("1"), &a_socket_path());

        // Then we do not steal another process's inherited fds
        assert_eq!(source, SocketSource::SelfBind(a_socket_path()));
    }

    #[test]
    fn self_binds_when_systemd_reports_zero_activation_fds() {
        // Given LISTEN_PID is us but the passed-fd count is zero
        let my_pid = 4242;

        // When
        let source = resolve_socket_source(my_pid, Some("4242"), Some("0"), &a_socket_path());

        // Then there is nothing to adopt, so we bind ourselves
        assert_eq!(source, SocketSource::SelfBind(a_socket_path()));
    }

    #[test]
    fn self_binds_when_the_activation_environment_is_malformed() {
        // Given a non-numeric LISTEN_FDS value
        let my_pid = 4242;

        // When
        let source =
            resolve_socket_source(my_pid, Some("4242"), Some("not-a-number"), &a_socket_path());

        // Then we treat it as no activation and bind ourselves
        assert_eq!(source, SocketSource::SelfBind(a_socket_path()));
    }
}
