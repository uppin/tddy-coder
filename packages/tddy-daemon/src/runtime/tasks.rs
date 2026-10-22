use crate::config::DaemonConfig;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tddy_daemon_livekit::common_room_supervisor::CommonRoomSupervisorTask;
use teloxide::prelude::Bot;
use tokio::sync::Mutex;

/// Everything a running daemon spawned that must not outlive it.
///
/// Named rather than left inline in the signal handler because a shutdown that reaches only some
/// of these is indistinguishable, from the outside, from one that reached all of them — which is
/// how the workspace jails came to be missed entirely
/// (`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`).
#[derive(Clone)]
pub struct DaemonChildren {
    pub(super) cli_sessions: Arc<tddy_session_lifecycle::cli_session_manager::CliSessionManager>,
    pub(super) session_host:
        Option<Arc<tddy_session_lifecycle::connection_service::DaemonSessionHost>>,
    pub(super) index_daemon: Option<crate::index_daemon::IndexDaemonRegistry>,
}

impl DaemonChildren {
    /// Stop all of them. Idempotent, so the SIGTERM path and the graceful return can both call it.
    pub async fn shut_down(&self) {
        match &self.session_host {
            // Reaches the CLI sessions *and* the workspace jails — a jail is a
            // `tddy-sandbox-runner` holding a checkout open, and the in-process registry is its
            // only holder, so a daemon that exits without this leaves it running on the host.
            Some(host) => host.shut_down_children().await,
            None => self.cli_sessions.kill_all().await,
        }
        // Stopped explicitly rather than left to `tasks.abort_all()`: aborting the reaper that
        // would have stopped it is exactly how a sandbox runner gets orphaned.
        if let Some(index_daemon) = &self.index_daemon {
            index_daemon.shutdown().await;
        }
    }
}

/// The listeners, dialers and background loops an assembled runtime needs, none of them started.
///
/// [`build`] returns them instead of spawning them so that assembling a runtime binds nothing and
/// joins nothing; the host calls [`spawn`](RuntimeTasks::spawn) once it is ready to accept traffic.
pub struct RuntimeTasks {
    pub(super) common_room: Option<CommonRoomSupervisorTask>,
    pub(super) oauth_loopback_tunnel: Option<OauthLoopbackTunnel>,
    pub(super) local_socket: Option<LocalSocketTransport>,
    /// The agent-facing tool socket an **embedded** daemon serves, so a co-located agent spawned
    /// beside a jailed checkout can reach `ExecuteTool` and the roster at all. `None` on the
    /// binary host, which already serves both HTTP and the tonic local socket.
    pub(super) agent_tool_socket: Option<(PathBuf, Vec<tddy_rpc::ServiceEntry>)>,
    pub(super) lsp_idle_reaper: Option<tddy_lsp::LspRegistry>,
    pub(super) index_daemon: Option<crate::index_daemon::IndexDaemonRegistry>,
    pub(super) relay_idle_monitor: Option<(
        Arc<tddy_session_lifecycle::relay_idle::IdleTimeoutTracker>,
        tokio::sync::oneshot::Sender<()>,
    )>,
    pub(super) telegram_inbound: Option<TelegramInbound>,
    /// `#keyring` 6/9's peer-join watch, when a common room and an open-vault population exist to
    /// sync over. See `credential_sync::run_publish_watch`.
    pub(super) credential_sync_publish_watch: Option<CredentialSyncPublishWatch>,
}

/// What `#keyring` 6/9's peer-join watch needs to run, assembled in [`build`] and started by
/// [`RuntimeTasks::spawn`] like every other background loop here.
pub(super) struct CredentialSyncPublishWatch {
    pub(super) registry: Arc<tddy_daemon_livekit::livekit_peer_discovery::CommonRoomPeerRegistry>,
    pub(super) vaults: Arc<tddy_daemon_auth::SessionVaults>,
    pub(super) config: DaemonConfig,
    pub(super) engine: Arc<std::sync::Mutex<tddy_credential_sync::SyncEngine>>,
}

/// The OAuth loopback TCP proxy, which follows the common room across a reconnect rather than
/// belonging to any one connection — see [`tddy_daemon_livekit::oauth_loopback_tunnel::spawn_oauth_loopback_tunnel`].
pub(super) struct OauthLoopbackTunnel {
    pub(super) config: Arc<DaemonConfig>,
    pub(super) room_slot: Arc<tokio::sync::RwLock<Option<Arc<livekit::Room>>>>,
}

/// Join `livekit.common_room`, keep [`CommonRoomPeerRegistry`] in sync, and run the OAuth loopback
/// tunnel supervisor alongside — for an embedder that owns both for the lifetime of its process.
///
/// [`CommonRoomPeerRegistry`]: tddy_daemon_livekit::livekit_peer_discovery::CommonRoomPeerRegistry
///
/// The daemon itself does not use it: its discovery loop is owned by
/// [`tddy_daemon_livekit::common_room_supervisor`], which stops and restarts it when the configured room changes,
/// while the tunnel supervisor follows `room_slot` across that and so is started once. It composes
/// a loop from `tddy-daemon-livekit` with a supervisor from `tddy-daemon-auth`, and this is the
/// only crate that has both — which is why it is assembly and lives here rather than with either
/// half.
pub fn spawn_common_room_discovery_task(
    config: Arc<DaemonConfig>,
    registry: Arc<tddy_daemon_livekit::livekit_peer_discovery::CommonRoomPeerRegistry>,
    room_slot: Arc<tokio::sync::RwLock<Option<Arc<livekit::Room>>>>,
    signing_key: tddy_daemon_livekit::AdvertisedSigningKey,
) {
    // An embedder keeps no status board, so the loop reports on one nobody reads.
    let status = tddy_daemon_extensions::IntegrationStatusBoard::new()
        .reporter(tddy_daemon_livekit::LIVEKIT_INTEGRATION_ID, "LiveKit");
    tddy_daemon_livekit::oauth_loopback_tunnel::spawn_oauth_loopback_tunnel(
        &config,
        room_slot.clone(),
    );
    tddy_daemon_livekit::livekit_peer_discovery::spawn_common_room_discovery_loop(
        config,
        registry,
        room_slot,
        signing_key,
        status,
    );
}

/// Every family reachable on the binary's local Unix-domain socket, in one bundle.
pub(super) type BinaryLocalSocketServices = crate::local_socket_server::LocalSocketServices<
    tddy_session_lifecycle::SessionServiceImpl<
        tddy_session_lifecycle::connection_service::DaemonSessionHost,
    >,
    tddy_projects::ProjectServiceImpl<tddy_daemon_rpc::ProjectRpcHandler>,
    tddy_session_lifecycle::connection_service::DemoVmServiceImpl,
    tddy_daemon_livekit::LiveKitServiceImpl,
    tddy_host_service::HostServiceImpl,
    tddy_worktree_service::WorktreeServiceImpl,
    tddy_terminal_rpc::TerminalSessionServiceImpl,
    tddy_session_lifecycle::connection_service::PeerRoutedSessionAgents,
    tddy_session_lifecycle::connection_service::PeerRoutedActivity,
    tddy_discovery::CatalogServiceImpl<tddy_daemon_rpc::CatalogRpcHandler>,
    tddy_tool_engine::ExecToolServiceImpl<tddy_daemon_rpc::ExecToolRpcHandler>,
    tddy_session_lifecycle::pr_stack_rpc::PrStackServiceImpl<tddy_daemon_rpc::PrStackRpcHandler>,
>;

/// The local Unix-domain-socket `ConnectionService` transport (SO_PEERCRED peer-trust plus
/// `MintLocalToken`). Only the binary host serves it — the socket path is per-daemon, and a
/// systemd-activated listener is addressed to the binary's pid.
pub(super) struct LocalSocketTransport {
    pub(super) socket_path: PathBuf,
    /// Node 1 added hosts and worktrees, node 6 the terminal family and node 7 the session-agent
    /// and activity families; node 8 catalog, exec tools and PR stack. A caller that reached a
    /// method here before its split must go on reaching it here after. Adapters are generated by
    /// `tddy-codegen`, not hand-written.
    pub(super) services: BinaryLocalSocketServices,
}

/// The Telegram bot's inbound dispatcher: the buttons an operator presses in a chat.
pub(super) struct TelegramInbound {
    pub(super) bot: Bot,
    pub(super) harness: Arc<
        Mutex<
            tddy_telegram_control::telegram_session_control::TelegramSessionControlHarness<
                tddy_telegram_control::telegram_notifier::TeloxideSender,
            >,
        >,
    >,
}

/// The tasks a host started, so it can stop them when its server is done.
pub struct RuntimeTaskHandles {
    handles: Vec<tokio::task::JoinHandle<()>>,
}

impl RuntimeTaskHandles {
    /// Abort every task this runtime started.
    pub fn abort_all(&self) {
        for handle in &self.handles {
            handle.abort();
        }
    }
}

impl RuntimeTasks {
    /// Start every listener, dialer and background loop this runtime needs. Must be called from
    /// within the host's async runtime.
    pub fn spawn(self) -> RuntimeTaskHandles {
        let mut handles = Vec::new();

        if let Some(tunnel) = self.oauth_loopback_tunnel {
            if let Some(handle) =
                tddy_daemon_livekit::oauth_loopback_tunnel::spawn_oauth_loopback_tunnel(
                    &tunnel.config,
                    tunnel.room_slot,
                )
            {
                handles.push(handle);
            }
        }

        if let Some(reaper) = self.lsp_idle_reaper {
            handles.push(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(Duration::from_secs(60));
                ticker.tick().await; // consume the immediate first tick
                loop {
                    ticker.tick().await;
                    reaper.reap_idle().await;
                }
            }));
        }

        // The index daemon's idle reaper, on the same 60-second cadence as the LSP one above. It
        // only ever *stops* a process: starting one is a caller's doing, and a daemon that indexed
        // on a timer would hold a multi-gigabyte rust-analyzer for a host nobody was restructuring.
        if let Some(index_daemon) = self.index_daemon {
            handles.push(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(Duration::from_secs(60));
                ticker.tick().await; // consume the immediate first tick
                loop {
                    ticker.tick().await;
                    index_daemon.reap_idle().await;
                }
            }));
        }

        if let Some((socket_path, entries)) = self.agent_tool_socket {
            handles.push(tokio::spawn(async move {
                // Runs for the life of the process: an embedded daemon is stopped by its
                // application exiting, and the socket file is removed on the way out.
                let shutdown = std::future::pending::<()>();
                if let Err(e) = crate::agent_tool_socket::serve_agent_tool_socket(
                    &socket_path,
                    entries,
                    shutdown,
                )
                .await
                {
                    log::error!(
                        target: "tddy_daemon::agent_tool_socket",
                        "agent tool socket exited with error: {e:#}"
                    );
                }
            }));
        }

        if let Some(local_socket) = self.local_socket {
            handles.push(tokio::spawn(async move {
                let shutdown = async {
                    let mut term =
                        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                            .ok();
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {}
                        _ = async {
                            match term.as_mut() {
                                Some(s) => { s.recv().await; }
                                None => std::future::pending::<()>().await,
                            }
                        } => {}
                    }
                };
                if let Err(e) = crate::local_socket_server::serve_connection_uds(
                    &local_socket.socket_path,
                    local_socket.services,
                    shutdown,
                )
                .await
                {
                    log::error!(
                        target: "tddy_daemon::local_socket_server",
                        "local socket server exited with error: {e:#}"
                    );
                }
            }));
        }

        if let Some((tracker, trigger)) = self.relay_idle_monitor {
            handles.push(tokio::spawn(async move {
                let check_interval = Duration::from_secs(30);
                loop {
                    tokio::time::sleep(check_interval).await;
                    if tracker.should_shutdown() {
                        log::info!(
                            "relay daemon: idle timeout expired — triggering graceful shutdown"
                        );
                        let _ = trigger.send(());
                        return;
                    }
                }
            }));
        }

        if let Some(watch) = self.credential_sync_publish_watch {
            handles.push(tokio::spawn(crate::credential_sync::run_publish_watch(
                watch.registry,
                watch.vaults,
                watch.config,
                watch.engine,
            )));
        }

        if let Some(inbound) = self.telegram_inbound {
            handles.push(tokio::spawn(async move {
                if let Err(e) = tddy_telegram_control::telegram_bot::run_telegram_bot(
                    inbound.bot,
                    inbound.harness,
                )
                .await
                {
                    log::warn!(
                        target: "tddy_daemon::telegram_bot",
                        "inbound dispatcher ended: {e:#}"
                    );
                }
            }));
        }

        // The common-room connection, from now until the daemon's roster is dropped: it joins the
        // configured room and rejoins whenever `daemon_config.DaemonConfigService` names another.
        if let Some(supervisor) = self.common_room {
            handles.push(tokio::spawn(supervisor.run()));
        }

        RuntimeTaskHandles { handles }
    }
}
