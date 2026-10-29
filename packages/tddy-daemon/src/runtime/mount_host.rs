//! Mounting the session host: the services it is built from, the host itself, and the
//! code-intelligence executors registered against its task registry.

use async_trait::async_trait;
use tokio::sync::broadcast;

use super::build_context::BuildContext;
use super::mount::{Hosted, Mount, Served, Shared};
use super::*;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::session_notifications::{
    SessionNotification, SessionNotificationSubscriber,
};

/// Offers the bus a subscriber a host registered on [`RuntimeOptions`]. The bus takes its
/// subscribers by value and sized, while a host holds its own as `Arc<dyn …>`, so the bus gets this
/// forwarder over that `Arc`, and every method is passed through unchanged.
struct RegisteredSubscriber(Arc<dyn SessionNotificationSubscriber>);

#[async_trait]
impl SessionNotificationSubscriber for RegisteredSubscriber {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn wants(&self, notification: &SessionNotification) -> bool {
        self.0.wants(notification)
    }

    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        self.0.deliver(notification).await
    }

    fn client_relay(&self) -> Option<broadcast::Receiver<SessionNotification>> {
        self.0.client_relay()
    }
}

impl BuildContext {
    /// The daemon's operator channel: Telegram's hooks when a bot is configured, the port's null
    /// object otherwise. One channel for both the session host and the notification bus.
    fn operator_channel(&self) -> tddy_daemon_kernel::operator_channel::SharedOperatorChannel {
        match &self.telegram_hooks {
            Some(hooks) => {
                Arc::clone(hooks) as tddy_daemon_kernel::operator_channel::SharedOperatorChannel
            }
            None => Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        }
    }

    pub(super) async fn open_shared(&self) -> anyhow::Result<Shared> {
        let config = &self.config;
        let tddy_data_dir = &self.identity.keys.tddy_data_dir;
        // This daemon's model registry: the providers it talks to, their models, and the
        // assistants composed from them. Opened before ConnectionService so an assistant is
        // listed by `ListAgents` as a selectable `--agent`.
        let model_registry = Arc::new(
            tddy_model_registry::ModelRegistryStore::open(
                &tddy_data_dir.join("models.db"),
                &tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(config),
                // The same directory `DaemonSessionHost` resolves YAML defs from, so an
                // assistant cannot be created under a name one of them already answers to.
                &tddy_data_dir.join("agents"),
            )
            .await?
            .reserving_agent_ids(config.allowed_agents().iter().map(|a| a.id.clone())),
        );
        // The daemon's session-notification bus: the operator channel (Telegram's, on a daemon
        // that has one) takes the attention-worthy events from the activity-status path, and
        // `StreamSessionNotifications` relays every event to the browsers driving the drawer's
        // indicators. Assembled here because `DaemonSessionHost::new` installs no bus of its own,
        // and because the stream subscriber must be the very one the RPC handler subscribes to.
        let session_notification_bus = {
            let bus = tddy_session_lifecycle::session_notifications::SessionNotificationBus::new()
                .with_subscriber(Arc::new(
                    tddy_session_lifecycle::session_notifications::OperatorChannelSubscriber::new(
                        self.operator_channel(),
                    ),
                ));
            let bus = bus.with_subscriber(Arc::new(
                tddy_session_lifecycle::session_notification_subscribers::SessionNotificationStreamSubscriber::new(),
            ));
            // The subscribers an embedding host registered, after both built-ins in registration order.
            let bus = self
                .options
                .notification_subscribers
                .iter()
                .fold(bus, |bus, registered| {
                    bus.with_subscriber(Arc::new(RegisteredSubscriber(Arc::clone(registered))))
                });
            Arc::new(bus)
        };
        // One registry for the whole daemon. `ConnectionService` streams and answers prompts;
        // `ScreenSharingService` raises them for a desktop password. A prompt raised on one
        // instance and answered on another is a question nobody can answer, so both are handed
        // this one.
        let host_prompts: Arc<dyn tddy_session_lifecycle::host_prompts::HostPromptRegistry> =
            Arc::new(tddy_session_lifecycle::host_prompts::InMemoryHostPromptRegistry::new());
        Ok(Shared {
            model_registry,
            session_notification_bus,
            host_prompts,
        })
    }

    pub(super) fn open_served(&self, mount: &Mount, shared: &Shared) -> Served {
        let config = &self.config;
        let tddy_data_dir = &self.identity.keys.tddy_data_dir;
        let user_resolver = &mount.user_resolver;
        let host_registry = &self.identity.keys.host_registry;
        let host_prompts = &shared.host_prompts;
        let livekit_discovery = &mount.livekit_discovery;
        let idle_tracker = &self.idle_tracker;
        let shared_session_rooms = &self.registries.shared_session_rooms;
        // Built before `DaemonSessionHost` consumes `livekit_discovery`: the host service
        // routes on the same live roster and forwards over the same common room, so both take a
        // handle on the one set of discovery handles rather than each deriving its own.
        let host_service_impl = {
            let mut host_service = tddy_host_service::HostServiceImpl::new(
                config.clone(),
                tddy_data_dir,
                Arc::clone(user_resolver),
            )
            .with_host_registry(Arc::clone(host_registry))
            .with_host_prompts(Arc::clone(host_prompts));
            if let Some(ref handles) = livekit_discovery {
                host_service = host_service
                    .with_eligible_daemon_source(Arc::clone(&handles.eligible_daemon_source))
                    .with_common_room(tddy_daemon_kernel::peer_forwarding::CommonRoom::new(
                        tddy_daemon_livekit::LiveKitPeerForwarder::new(Arc::clone(
                            &handles.common_room_livekit_room,
                        )),
                        config,
                    ));
            }
            if let Some(ref tracker) = idle_tracker {
                host_service = host_service.with_idle_tracker(tracker.clone());
            }
            Arc::new(host_service)
        };

        // The rooms this daemon hosts per worktree, closed by path before a checkout is removed.
        // Injected as the port `tddy-worktree-service` declares rather than the registry itself:
        // a session room is family C and stays in this crate.
        let worktree_service_impl = {
            let mut worktree_service = tddy_worktree_service::WorktreeServiceImpl::new(
                config.clone(),
                tddy_data_dir.clone(),
                Arc::clone(user_resolver),
            )
            .with_session_rooms(
                Arc::clone(shared_session_rooms) as Arc<dyn tddy_daemon_extensions::SessionRooms>
            );
            if let Some(ref tracker) = idle_tracker {
                worktree_service = worktree_service.with_idle_tracker(tracker.clone());
            }
            Arc::new(worktree_service)
        };
        Served {
            host_service_impl,
            worktree_service_impl,
        }
    }

    pub(super) fn manage_index_daemon(&mut self, config_arc: &Arc<DaemonConfig>) {
        let shared_claude_cli_manager = &self.registries.shared_claude_cli_manager;
        // The warm code-intelligence index, when this daemon was configured to manage one. Its
        // lifecycle is owned here and its *index* is owned by the process itself — the two are
        // split because rust-analyzer's handshake is fixed at spawn and this daemon's registry
        // advertises no client capabilities, so the two cannot share one registry entry (see the
        // PRD, § "A separate process, whose lifecycle the daemon manages"). Nothing is started
        // here: the first request that needs it starts it.
        {
            // The section is an override, not a switch: without it the defaults apply.
            let configured = &config_arc.index_daemon.clone().unwrap_or_default();
            let index_daemon = crate::index_daemon::IndexDaemonRegistry::new(
                crate::index_daemon::IndexDaemonSpawn::from_config(configured),
                shared_claude_cli_manager.task_registry(),
            );
            log::info!(
                target: "tddy_daemon::index_daemon",
                "managing an index daemon on {}",
                index_daemon.socket_path().display()
            );
            self.tasks.index_daemon = Some(index_daemon.clone());
            self.index_daemon_registry = Some(index_daemon);
        }
    }

    pub(super) async fn bind_askpass(
        &self,
        shared: &Shared,
        served: &Served,
    ) -> anyhow::Result<Arc<tddy_host_service::askpass::AskpassEndpoint>> {
        let host_prompts = &shared.host_prompts;
        let host_service_impl = &served.host_service_impl;
        let askpass_endpoint = Arc::new(
            tddy_host_service::askpass::AskpassEndpoint::start(
                Arc::clone(host_prompts),
                host_service_impl.host_keypair(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("bind the askpass endpoint on loopback: {e}"))?,
        );
        Ok(askpass_endpoint)
    }

    pub(super) fn build_session_host(
        &self,
        mount: &mut Mount,
        shared: &Shared,
        served: &Served,
        askpass_endpoint: Arc<tddy_host_service::askpass::AskpassEndpoint>,
    ) -> DaemonSessionHost {
        let config = &self.config;
        let options = &self.options;
        let tddy_data_dir = &self.identity.keys.tddy_data_dir;
        let sessions_base_resolver = &mount.sessions_base_resolver;
        let user_resolver = mount.user_resolver.clone();
        let livekit_discovery = mount.livekit_discovery.take();
        let shared_claude_cli_manager = &self.registries.shared_claude_cli_manager;
        let shared_session_rooms = &self.registries.shared_session_rooms;
        let shared_session_admissions = &self.registries.shared_session_admissions;
        let room_tokens = &self.identity.keys.room_tokens;
        let livekit_reach = &self.plugins.livekit_reach;
        let terminal_exposure = &self.plugins.terminal_exposure;
        let split_rooms = Arc::clone(&self.plugins.split_rooms);
        let model_registry = &shared.model_registry;
        let session_notification_bus = Arc::clone(&shared.session_notification_bus);
        let session_tokens = &mount.session_tokens;
        // Before the host serves, so no conversation of this run is among them: what this daemon's last
        // run left of its headless conversations — their MCP configs carry agent credentials.
        sweep_headless_files(config, tddy_data_dir);
        // One registry for the whole host: the launcher binds each headless conversation's approvals
        // into it, and the host answers those approvals through it.
        let approval_routes = Arc::default();
        let connection_impl = tddy_session_lifecycle::connection_service::DaemonSessionHost::new(
            config.clone(),
            sessions_base_resolver.clone(),
            tddy_data_dir.clone(),
            user_resolver.clone(),
            options.spawn_client.clone(),
            options.spawn_backend.clone(),
            tddy_daemon_livekit::peer_routing::peer_routing_over(config, livekit_discovery),
            self.operator_channel(),
            Arc::clone(shared_claude_cli_manager),
        );

        crate::livekit_host_ports::with_livekit_room_ports(
        connection_impl,
        Arc::clone(shared_session_rooms),
        Arc::clone(shared_session_admissions),
        Arc::clone(room_tokens),
    )
        .with_session_reach(
            Arc::clone(livekit_reach) as Arc<dyn tddy_daemon_extensions::SessionReach>
        )
        .with_terminal_exposure(Arc::clone(terminal_exposure))
        // The worktree service's own calculator, so the project-home worktree tools list the same
        // calculation state the Worktrees screen streams.
        .with_worktree_size_calculator(served.worktree_service_impl.worktree_size_calculator())
        .with_split_rooms(split_rooms)
        .with_model_registry(Arc::clone(model_registry))
        .with_coding_agent_launcher(Arc::new(
            tddy_session_lifecycle::coding_agent_launcher::DaemonCodingAgentLauncher::for_daemon(
                tddy_session_lifecycle::coding_agent_launcher::DaemonLauncherDeps {
                    config: config.clone(),
                    tddy_data_dir: tddy_data_dir.clone(),
                    spawn_backend: options.spawn_backend.clone(),
                    vaults: self.identity.credential_vaults.clone(),
                    user_resolver,
                    tokens: Some(session_tokens.clone()),
                },
            )
            .with_approval_routes(Arc::clone(&approval_routes)),
        ))
        .with_session_notification_bus(session_notification_bus)
        .with_session_tokens(session_tokens.clone())
        .with_askpass_endpoint(askpass_endpoint)
        .with_approval_routes(approval_routes)
        // Read here, before the local socket server is spawned: adopting its own listener
        // clears the `LISTEN_*` variables the host sockets are announced in.
        .with_inherited_host_session_sockets(
            tddy_session_lifecycle::connection_service::inherited_host_sockets::adopt_from_environment(),
        )
    }

    pub(super) fn install_session_host(
        &mut self,
        mut connection_impl: DaemonSessionHost,
    ) -> Hosted {
        let index_daemon_registry = &self.index_daemon_registry;
        let idle_tracker = &self.idle_tracker;
        let shared_session_admissions = &self.registries.shared_session_admissions;
        // A started session's worktree starts its warm-up in the background; the host cannot name
        // this crate, so it is told through its observer port. Without an index daemon there is
        // nothing to warm, so nothing is installed.
        if let Some(registry) = index_daemon_registry.clone() {
            connection_impl = connection_impl.with_worktree_observer(Arc::new(
                tddy_daemon_rpc::code_index_warmup::IndexWarmupObserver::new(Arc::new(registry)
                    as Arc<dyn tddy_daemon_rpc::code_navigation::IndexChannelSource>)
                .with_supports(Arc::clone(&self.index_supports)),
            ));
        }
        if let Some(ref tracker) = idle_tracker {
            connection_impl = connection_impl.with_idle_tracker(tracker.clone());
        }
        if let Some(vaults) = self.identity.credential_vaults.clone() {
            tddy_daemon_auth::vault_lifetimes::spawn_credential_sweep(&vaults);
            connection_impl = connection_impl.with_credential_vaults(vaults);
        }
        // The families served above the host are built from it and installed on it **last**:
        // they share its `Arc`s, so a `with_*` after this line would leave them holding the value
        // it replaced (`tddy_daemon_rpc::families`).
        let (connection_impl, rpc_handlers) =
            tddy_daemon_rpc::RpcHandlers::install(connection_impl);
        // Share one instance across transports: the LiveKit/HTTP RpcService server and the
        // local Unix-domain-socket tonic server both reference the same Arc, so a session
        // started over the socket is visible over every other transport.
        let connection_arc = Arc::new(connection_impl);
        connection_arc.install_sandbox_rpc_bridge();
        self.session_host = Some(Arc::clone(&connection_arc));
        // The periodic project-home auto-stop reaps through the same host the sessions run on.
        self.tasks.project_home_reaper = Some(Arc::clone(&connection_arc));
        // Get the shared TaskRegistry before handing the impl to the servers.
        let task_registry = connection_arc.task_registry();
        // The admission registry is shared with the SessionAdmissionService served on the
        // common room (PRD § "What attach does" step 3); capture it before `connection_arc`
        // moves into the ConnectionServiceServer below.
        let session_admissions = Arc::clone(shared_session_admissions);
        Hosted {
            connection_arc,
            rpc_handlers,
            task_registry,
            session_admissions,
        }
    }

    pub(super) fn register_code_intelligence(&mut self, task_registry: &tddy_task::TaskRegistry) {
        let index_daemon_registry = &self.index_daemon_registry;
        // Register the build-catalog provider so a populated session catalog includes the
        // repository's `BUILD.yaml` targets (discovery lives in `tddy-bsp` on top of
        // `tddy-build`; `tddy-core` owns only the port).
        tddy_bsp::register_catalog_provider();

        // Reusable-LSP executor: a Rust-only executor sharing this daemon's task registry,
        // so `Lsp*` tool calls (relayed through tddy-tool-engine) resolve to a real, reused
        // language server; the loop that reaps servers left idle is the host's to start.
        //
        // With an `index_daemon:` section the session's `Lsp*` tools ask the managed index instead
        // (the registry above), so no second rust-analyzer is started; this executor then answers
        // nothing and its registry, which the reaper drives, stays empty.
        let lsp_executor = tddy_lsp_executor::TddyLspExecutor::new(
            tddy_lsp::LspAllowList::rust_only(),
            task_registry.clone(),
            Duration::from_secs(300),
        );
        self.tasks.lsp_idle_reaper = Some(lsp_executor.registry());
        let index_channel = index_daemon_registry.clone().map(|registry| {
            Arc::new(registry) as Arc<dyn tddy_lsp_executor::index_backed::IndexChannel>
        });
        // The same index answers the session's `restructure_*` tools. Without `index_daemon:`
        // nothing is registered, so the tools are not advertised to any session.
        if let Some(index_channel) = index_channel.clone() {
            tddy_core::toolcall::restructure::register_restructure_executor(Arc::new(
                tddy_lsp_executor::restructure_via_index::IndexRestructureExecutor::new(
                    index_channel,
                ),
            ));
        }
        tddy_core::toolcall::lsp::register_lsp_executor(
            tddy_lsp_executor::index_backed::select_lsp_executor(
                index_channel,
                Arc::new(lsp_executor),
            ),
        );
    }
}

/// Remove the headless conversation files this daemon's previous run left behind. A failure is
/// logged, not fatal: the files are a previous run's, and nothing this run serves depends on them.
fn sweep_headless_files(
    config: &tddy_daemon_kernel::config::DaemonConfig,
    tddy_data_dir: &std::path::Path,
) {
    match tddy_session_lifecycle::coding_agent_launcher::sweep_headless_files_left_by_this_daemon(
        config,
        tddy_data_dir,
    ) {
        Ok(swept) if swept.is_empty() => {}
        Ok(swept) => log::info!(
            "removed the headless files of {} conversation(s) a previous run left: {}",
            swept.len(),
            swept.join(", ")
        ),
        Err(e) => log::warn!(
            "could not sweep the headless conversation files under {}: {e}",
            tddy_data_dir.display()
        ),
    }
}
