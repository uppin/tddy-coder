//! The state `runtime::build` threads through its stages: the identity it resolved, the registries
//! and plugins it assembled, and the tasks it hands back unstarted.

use super::identity::Identity;
use super::integrations::{Plugins, Registries, RelayIdle, Telegram};
use super::*;
use tddy_daemon_livekit::livekit_peer_discovery::CommonRoomPeerRegistry;
use tddy_telegram_control::telegram_session_subscriber::TelegramDaemonHooks;

/// The registry and room slot `#keyring` 6/9's sync engine is built over.
pub(super) type CredentialSyncRoom = (
    Arc<CommonRoomPeerRegistry>,
    Arc<tokio::sync::RwLock<Option<Arc<livekit::Room>>>>,
);

// TODO(restructure-engine): hand-written (D1) — the context struct that lets `build`'s groups
// extract into methods that stay within the five-parameter limit.
pub(super) struct BuildContext {
    pub(super) config: DaemonConfig,
    pub(super) options: RuntimeOptions,
    pub(super) identity: Identity,
    pub(super) registries: Registries,
    pub(super) plugins: Plugins,
    pub(super) telegram_hooks: Option<Arc<TelegramDaemonHooks>>,
    pub(super) lifecycle_telegram: Option<(DaemonConfig, Arc<dyn TelegramSender + Send + Sync>)>,
    pub(super) idle_tracker: Option<Arc<tddy_session_lifecycle::relay_idle::IdleTimeoutTracker>>,
    pub(super) relay_shutdown: Option<tokio::sync::oneshot::Receiver<()>>,
    /// The peer-discovery handles the roster is built from, when this daemon assembles discovery at
    /// all. The common-room supervisor reconnects *these*, so the roster keeps pointing at the
    /// registry and room slot it was built with.
    pub(super) peer_discovery: Option<PeerDiscoveryHandles>,
    /// The same registry and room slot, captured for `#keyring` 6/9's sync engine — which needs a
    /// room to sync over regardless of whether `keyring.group_secret` is set (see
    /// `credential_sync`'s module doc). `None` whenever `peer_discovery` is, since there is nothing
    /// to sync over without a common room.
    pub(super) credential_sync_room: Option<CredentialSyncRoom>,
    /// The index daemon this runtime manages, so the host can stop it. Assembled below, inside the
    /// branch that has the task registry to run it on.
    pub(super) index_daemon_registry: Option<crate::index_daemon::IndexDaemonRegistry>,
    /// The languages the index daemon routes — the registry it builds for itself — which the
    /// session level asks whether a worktree has an index (`#reshape` extension-language-server,
    /// R7): an extension-only worktree whose language has a configured server has one.
    pub(super) index_supports: Arc<tddy_daemon_rpc::code_navigation::SupportRegistry>,
    /// The session host, so the runtime's own host can reach the children it holds at shutdown —
    /// notably the workspace jails, each a `tddy-sandbox-runner` process nothing else would stop
    /// (`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`). `None`
    /// for a daemon built without authentication, which hosts no sessions to have children.
    pub(super) session_host:
        Option<Arc<tddy_session_lifecycle::connection_service::DaemonSessionHost>>,
    pub(super) tasks: RuntimeTasks,
}

impl BuildContext {
    pub(super) fn new(
        config: DaemonConfig,
        options: RuntimeOptions,
        identity: Identity,
        index_supports: Arc<tddy_daemon_rpc::code_navigation::SupportRegistry>,
    ) -> Self {
        let registries = Registries::open(&config, identity.user_resolver.as_ref());
        let plugins = Plugins::open(
            &config,
            &registries.shared_session_rooms,
            &identity.keys.room_tokens,
        );
        let telegram = Telegram::open(
            &config,
            &options,
            &identity.keys,
            &registries,
            &plugins.terminal_exposure,
        );
        let relay = RelayIdle::open(&options);
        let tasks = RuntimeTasks {
            common_room: None,
            oauth_loopback_tunnel: None,
            local_socket: None,
            agent_tool_socket: None,
            lsp_idle_reaper: None,
            index_daemon: None,
            relay_idle_monitor: relay.relay_idle_monitor,
            telegram_inbound: telegram.inbound,
            credential_sync_publish_watch: None,
        };
        Self {
            config,
            options,
            identity,
            registries,
            plugins,
            telegram_hooks: telegram.hooks,
            lifecycle_telegram: telegram.lifecycle,
            idle_tracker: relay.idle_tracker,
            relay_shutdown: relay.relay_shutdown,
            peer_discovery: None,
            credential_sync_room: None,
            index_daemon_registry: None,
            index_supports,
            session_host: None,
            tasks,
        }
    }

    pub(super) fn mount_daemon_settings(&mut self) {
        let options = &self.options;
        let config = &self.config;
        let common_room = &self.registries.common_room;
        let config_service_authenticator = self.registries.config_service_authenticator.clone();
        let integration_board = Arc::clone(&self.plugins.integration_board);
        let rpc_entries = &mut self.identity.rpc_entries;
        // The daemon's own settings, read and written by its UI. Registered for every host — a desktop
        // application embedding the daemon is the one that most needs to configure it — and gated on the
        // same session token as every other daemon RPC.
        let config_service = DaemonConfigServiceServer::new(
            DaemonConfigServiceImpl::new(
                options.config_path.clone(),
                Arc::new(Mutex::new(config.clone())),
                Arc::clone(common_room) as Arc<dyn CommonRoomSupervisor>,
                config_service_authenticator,
            )
            .with_integration_status(integration_board),
        );
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: DaemonConfigServiceServer::<DaemonConfigServiceImpl>::NAME,
            service: Arc::new(config_service) as Arc<dyn tddy_rpc::RpcService>,
        });

        let service_name_strs: Vec<&str> = rpc_entries.iter().map(|e| e.name).collect();
        rpc_entries.push(tddy_service::reflection_entry_from_with_supplements(
            &service_name_strs,
            &[tddy_terminal_rpc::TERMINAL_SESSION_DESCRIPTOR_BYTES],
        ));
    }

    pub(super) fn serve_common_room(&mut self) {
        // Serve the daemon's RPC services on the LiveKit common room, so a client that can join the
        // room can invoke every service without an HTTP origin — and keep serving them on whatever
        // room the configuration names next. Joining is the host's to start: assembling a runtime
        // dials nothing.
        self.tasks.common_room = Some(self.registries.common_room.task(Arc::new(
            DaemonCommonRoomConnector::new(
                self.config.clone(),
                cloned_entries(&self.identity.rpc_entries),
                self.peer_discovery.take(),
            ),
        )));

        // An embedded daemon serves no HTTP listener and no tonic local socket, so this socket is the
        // only channel a *co-located agent* has to it — a separate OS process spawned beside a jailed
        // checkout, which can use neither the application's in-process bridge nor a port that only
        // answers the OAuth callback. Built from the finished roster, so a tool call over it reaches
        // the very handlers the UI does. See `crate::agent_tool_socket`.
        if self.options.host == RuntimeHost::Embedded {
            self.tasks.agent_tool_socket = Some((
                crate::agent_tool_socket::agent_tool_socket_path(&self.identity.keys.tddy_data_dir),
                cloned_entries(&self.identity.rpc_entries),
            ));
        }
    }

    pub(super) fn into_runtime(self) -> DaemonRuntime {
        DaemonRuntime {
            entries: self.identity.rpc_entries,
            config: self.config,
            cli_sessions: self.registries.shared_claude_cli_manager,
            session_host: self.session_host,
            lifecycle_telegram: self.lifecycle_telegram,
            relay_shutdown: self.relay_shutdown,
            index_daemon: self.index_daemon_registry,
            tasks: self.tasks,
            credential_vaults: self.identity.credential_vaults,
        }
    }
}
