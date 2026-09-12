//! Serve the daemon's local-socket services over a Unix-domain socket with tonic gRPC.
//!
//! The local socket is the peer-trust transport: tonic populates each request's `UdsConnectInfo`
//! with the caller's SO_PEERCRED credentials, which the [`ConnectionServiceTonicAdapter`] reads in
//! `MintLocalToken`. This is spawned as an independent task alongside the HTTP server; it shares
//! the same service instances (via `Arc`) so work started over the socket is visible over every
//! other transport.
//!
//! **Nine services, one socket.** `#unbundle` node 1 split hosts and worktrees out of
//! `connection.ConnectionService`, node 6 split the terminal family out after them, node 7 the
//! session-agent and activity families after that, and node 8 the catalog, exec-tool and PR-stack
//! families; a caller that reached any of them over this socket must go on reaching it over this
//! socket. One `Server::builder()` with nine `add_service` calls is what keeps that true — a second
//! socket would be a second address to configure, and a service left off this builder would answer
//! on every transport except the local one. The in-jail
//! `tddy-sandbox-app` is the caller that proves it for `terminal_session.TerminalSessionService`:
//! its whole terminal bridge is the bidi `StreamSessionTerminalIO`, dialled here and nowhere else.
//!
//! Node 7's two are the reason this list is worth stating rather than assuming. Five of
//! `session_agents.SessionAgentService`'s nine methods are what `tddy-sandbox-runner`'s relay
//! allowlist lets an in-jail agent reach on its host, so a coordinate missing from this builder is
//! not a lost feature but a capability disabled inside a jail — and it fails closed, silently, at
//! runtime. All 17 of their adapter methods are **generated** by `tddy-codegen`'s
//! `generate_tonic_adapter`, the same way the terminal family's nine are; nothing here is
//! hand-written.

use std::future::Future;
use std::os::unix::io::{FromRawFd, RawFd};
use std::path::{Path, PathBuf};

use anyhow::Context;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;

use tddy_service::proto::activity::{
    ActivityService as RpcActivityService, ActivityServiceTonicAdapter,
};
use tddy_service::proto::catalog::{
    CatalogService as RpcCatalogService, CatalogServiceTonicAdapter,
};
use tddy_service::proto::connection::ConnectionService as RpcConnectionService;
use tddy_service::proto::exec_tools::{
    ExecToolService as RpcExecToolService, ExecToolServiceTonicAdapter,
};
use tddy_service::proto::host::HostService as RpcHostService;
use tddy_service::proto::pr_stack::{
    PrStackService as RpcPrStackService, PrStackServiceTonicAdapter,
};
use tddy_service::proto::session_agents_svc::{
    SessionAgentService as RpcSessionAgentService, SessionAgentServiceTonicAdapter,
};
use tddy_service::proto::tonic_activity::activity_service_server::ActivityServiceServer;
use tddy_service::proto::tonic_catalog::catalog_service_server::CatalogServiceServer;
use tddy_service::proto::tonic_exec_tools::exec_tool_service_server::ExecToolServiceServer;
use tddy_service::proto::tonic_pr_stack::pr_stack_service_server::PrStackServiceServer;
use tddy_service::proto::tonic_session_agents::session_agent_service_server::SessionAgentServiceServer;
use tddy_service::proto::worktree::WorktreeService as RpcWorktreeService;
use tddy_service::tonic_connection::connection_service_server::ConnectionServiceServer;
use tddy_service::tonic_host::host_service_server::HostServiceServer;
use tddy_service::tonic_worktree::worktree_service_server::WorktreeServiceServer;
use tddy_terminal_rpc::proto::terminal_session::{
    TerminalSessionService as RpcTerminalSessionService, TerminalSessionServiceTonicAdapter,
};
use tddy_terminal_rpc::proto::tonic_terminal_session::terminal_session_service_server::TerminalSessionServiceServer;

use crate::connection_tonic_adapter::ConnectionServiceTonicAdapter;
use crate::host_tonic_adapter::HostServiceTonicAdapter;
use crate::worktree_tonic_adapter::WorktreeServiceTonicAdapter;

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

/// The nine adapters mounted on the one socket, passed as a bundle.
///
/// A bundle rather than nine positional parameters because the list only grows: every `#unbundle`
/// node that takes a family out of `connection.ConnectionService` adds one, and a caller that has
/// to get nine same-shaped arguments in the right order is a caller that can silently swap two.
pub struct LocalSocketServices<C, H, W, T, S, A, Cat, E, P> {
    /// Reads the caller's SO_PEERCRED credentials in `MintLocalToken`; the reason this transport
    /// exists at all.
    pub connection: ConnectionServiceTonicAdapter<C>,
    /// The two families `#unbundle` node 1 split out.
    pub host: HostServiceTonicAdapter<H>,
    pub worktree: WorktreeServiceTonicAdapter<W>,
    /// Node 6's terminal family — the in-jail `tddy-sandbox-app` dials its bidi
    /// `StreamSessionTerminalIO` here and nowhere else.
    pub terminal: TerminalSessionServiceTonicAdapter<T>,
    /// Node 7's roster and conversations. Five of its nine methods are what `tddy-sandbox-runner`'s
    /// relay allowlist permits an in-jail agent to reach, so this one is a security boundary.
    pub session_agents: SessionAgentServiceTonicAdapter<S>,
    /// Node 7's activity, status, notifications and ACP replay.
    pub activity: ActivityServiceTonicAdapter<A>,
    /// Node 8's catalog — tools, agents, models and subagents.
    pub catalog: CatalogServiceTonicAdapter<Cat>,
    /// Node 8's exec-tool family. `ExecuteTool` is what `tddy-sandbox-runner`'s relay allowlist
    /// gates; a coordinate missing here fails closed inside a jail.
    pub exec_tools: ExecToolServiceTonicAdapter<E>,
    /// Node 8's PR-stack planning and branch resolution.
    pub pr_stack: PrStackServiceTonicAdapter<P>,
}

/// Bind `socket_path` and serve the nine local-socket services until `shutdown` resolves.
///
/// When launched via systemd socket activation (`LISTEN_PID`/`LISTEN_FDS` addressed to this
/// process), the inherited listener is adopted instead — systemd owns the socket node and its
/// permissions, so no directory is created, no stale file is unlinked, and no chmod is applied.
/// Otherwise a stale socket left by a previous run is unlinked first so the bind does not fail
/// with `EADDRINUSE`, and the parent directory is created if missing.
pub async fn serve_connection_uds<C, H, W, T, S, A, Cat, E, P>(
    socket_path: &Path,
    services: LocalSocketServices<C, H, W, T, S, A, Cat, E, P>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()>
where
    C: RpcConnectionService,
    H: RpcHostService,
    H::StreamHostPromptsStream: 'static,
    H::StreamHostStatsStream: 'static,
    W: RpcWorktreeService,
    W::StreamWorktreeStatsStream: 'static,
    W::StreamReadWorktreeFileStream: 'static,
    T: RpcTerminalSessionService,
    T::StreamSessionTerminalIoStream: 'static,
    T::StreamTerminalOutputStream: 'static,
    T::GetTerminalHistoryStream: 'static,
    T::WatchTerminalControlStream: 'static,
    S: RpcSessionAgentService,
    S::StreamSessionAgentsStream: 'static,
    S::PromptAgentConversationStream: 'static,
    A: RpcActivityService,
    A::StreamSessionActivityStream: 'static,
    A::StreamSessionNotificationsStream: 'static,
    A::StreamAgentActivityDeltaStream: 'static,
    A::StreamAcpReplayStream: 'static,
    Cat: RpcCatalogService,
    E: RpcExecToolService,
    E::StreamExecuteToolStream: 'static,
    P: RpcPrStackService,
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
                "local-socket services adopted systemd activation fd {fd} (socket label {})",
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
                "local-socket services listening on {}",
                path.display()
            );
            listener
        }
    };

    Server::builder()
        .add_service(ConnectionServiceServer::new(services.connection))
        .add_service(HostServiceServer::new(services.host))
        .add_service(WorktreeServiceServer::new(services.worktree))
        .add_service(TerminalSessionServiceServer::new(services.terminal))
        .add_service(SessionAgentServiceServer::new(services.session_agents))
        .add_service(ActivityServiceServer::new(services.activity))
        .add_service(CatalogServiceServer::new(services.catalog))
        .add_service(ExecToolServiceServer::new(services.exec_tools))
        .add_service(PrStackServiceServer::new(services.pr_stack))
        .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown)
        .await
        .context("serve the local-socket services")?;
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
