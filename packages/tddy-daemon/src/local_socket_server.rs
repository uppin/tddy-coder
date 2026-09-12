//! Serve the daemon's local-socket services over a Unix-domain socket with tonic gRPC.
//!
//! The local socket is the peer-trust transport: tonic populates each request's `UdsConnectInfo`
//! with the caller's SO_PEERCRED credentials, which [`LocalTokenUdsTonicAdapter`] reads in
//! `MintLocalToken`. This is spawned as an independent task alongside the HTTP server; it shares
//! the same service instances (via `Arc`) so work started over the socket is visible over every
//! other transport.
//!
//! **Twelve services, one socket.** `#unbundle` node 9 replaced the monolithic connection
//! coordinate with session, project, demo VM and local-token families; nodes 1, 6, 7 and 8 had
//! already split hosts, worktrees, terminal, session-agent, activity, catalog, exec-tool and
//! PR-stack. A caller that reached any method here before its split must go on reaching it here.

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
use tddy_service::proto::demo_vm::{
    DemoVmService as RpcDemoVmService, DemoVmServiceTonicAdapter,
};
use tddy_service::proto::exec_tools::{
    ExecToolService as RpcExecToolService, ExecToolServiceTonicAdapter,
};
use tddy_service::proto::host::HostService as RpcHostService;
use tddy_service::proto::pr_stack::{
    PrStackService as RpcPrStackService, PrStackServiceTonicAdapter,
};
use tddy_service::proto::project::{
    ProjectService as RpcProjectService, ProjectServiceTonicAdapter,
};
use tddy_service::proto::session::{
    SessionService as RpcSessionService, SessionServiceTonicAdapter,
};
use tddy_service::proto::session_agents_svc::{
    SessionAgentService as RpcSessionAgentService, SessionAgentServiceTonicAdapter,
};
use tddy_service::proto::tonic_activity::activity_service_server::ActivityServiceServer;
use tddy_service::proto::tonic_catalog::catalog_service_server::CatalogServiceServer;
use tddy_service::proto::tonic_demo_vm::demo_vm_service_server::DemoVmServiceServer;
use tddy_service::proto::tonic_exec_tools::exec_tool_service_server::ExecToolServiceServer;
use tddy_service::proto::tonic_local_token::local_token_service_server::LocalTokenServiceServer;
use tddy_service::proto::tonic_pr_stack::pr_stack_service_server::PrStackServiceServer;
use tddy_service::proto::tonic_project::project_service_server::ProjectServiceServer;
use tddy_service::proto::tonic_session::session_service_server::SessionServiceServer;
use tddy_service::proto::tonic_session_agents::session_agent_service_server::SessionAgentServiceServer;
use tddy_service::proto::worktree::WorktreeService as RpcWorktreeService;
use tddy_service::tonic_host::host_service_server::HostServiceServer;
use tddy_service::tonic_worktree::worktree_service_server::WorktreeServiceServer;
use tddy_terminal_rpc::proto::terminal_session::{
    TerminalSessionService as RpcTerminalSessionService, TerminalSessionServiceTonicAdapter,
};
use tddy_terminal_rpc::proto::tonic_terminal_session::terminal_session_service_server::TerminalSessionServiceServer;

use tddy_session_lifecycle::host_tonic_adapter::HostServiceTonicAdapter;
use tddy_session_lifecycle::local_token_tonic_adapter::LocalTokenUdsTonicAdapter;
use tddy_session_lifecycle::worktree_tonic_adapter::WorktreeServiceTonicAdapter;

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

/// Every adapter mounted on the one socket, passed as a bundle.
pub struct LocalSocketServices<Sess, Proj, Dm, H, W, T, Sa, A, Cat, E, P> {
    pub session: SessionServiceTonicAdapter<Sess>,
    pub project: ProjectServiceTonicAdapter<Proj>,
    pub demo_vm: DemoVmServiceTonicAdapter<Dm>,
    pub local_token: LocalTokenUdsTonicAdapter,
    pub host: HostServiceTonicAdapter<H>,
    pub worktree: WorktreeServiceTonicAdapter<W>,
    pub terminal: TerminalSessionServiceTonicAdapter<T>,
    pub session_agents: SessionAgentServiceTonicAdapter<Sa>,
    pub activity: ActivityServiceTonicAdapter<A>,
    pub catalog: CatalogServiceTonicAdapter<Cat>,
    pub exec_tools: ExecToolServiceTonicAdapter<E>,
    pub pr_stack: PrStackServiceTonicAdapter<P>,
}

/// Bind `socket_path` and serve the local-socket services until `shutdown` resolves.
pub async fn serve_connection_uds<Sess, Proj, Dm, H, W, T, Sa, A, Cat, E, P>(
    socket_path: &Path,
    services: LocalSocketServices<Sess, Proj, Dm, H, W, T, Sa, A, Cat, E, P>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()>
where
    Sess: RpcSessionService,
    Sess::StreamStartSessionStream: 'static,
    Proj: RpcProjectService,
    Dm: RpcDemoVmService,
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
    Sa: RpcSessionAgentService,
    Sa::StreamSessionAgentsStream: 'static,
    Sa::PromptAgentConversationStream: 'static,
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
            std::env::remove_var("LISTEN_PID");
            std::env::remove_var("LISTEN_FDS");
            std::env::remove_var("LISTEN_FDNAMES");

            // SAFETY: systemd guarantees fd `SD_LISTEN_FDS_START` is an open, listening
            // AF_UNIX socket when LISTEN_PID matches our pid and LISTEN_FDS >= 1.
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
        .add_service(SessionServiceServer::new(services.session))
        .add_service(ProjectServiceServer::new(services.project))
        .add_service(DemoVmServiceServer::new(services.demo_vm))
        .add_service(LocalTokenServiceServer::new(services.local_token))
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

    #[test]
    fn resolve_socket_source_self_bind_when_listen_pid_missing() {
        let path = PathBuf::from("/tmp/tddy-test.sock");
        assert_eq!(
            resolve_socket_source(42, None, Some("1"), &path),
            SocketSource::SelfBind(path)
        );
    }

    #[test]
    fn resolve_socket_source_self_bind_when_listen_pid_mismatch() {
        let path = PathBuf::from("/tmp/tddy-test.sock");
        assert_eq!(
            resolve_socket_source(42, Some("99"), Some("1"), &path),
            SocketSource::SelfBind(path)
        );
    }

    #[test]
    fn resolve_socket_source_adopts_when_listen_pid_matches() {
        let path = PathBuf::from("/tmp/tddy-test.sock");
        assert_eq!(
            resolve_socket_source(42, Some("42"), Some("1"), &path),
            SocketSource::Activated(SD_LISTEN_FDS_START)
        );
    }
}
