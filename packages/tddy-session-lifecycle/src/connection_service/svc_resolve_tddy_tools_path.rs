use crate::connection_service::agent_roster;
use tddy_daemon_livekit::livekit_rooms_stream::RoomRoster;
use tddy_spawn::spawn_worker;

use std::time::Duration;

use tddy_task::TaskRegistry;

use super::ROSTER_KEEPALIVE_INTERVAL;

use tddy_daemon_livekit::livekit_rooms_stream::room_roster_from_config;

use crate::multi_host::EligibleDaemonSource;

use crate::multi_host::LocalOnlyEligibleDaemonSource;

use crate::cli_session_manager::CliSessionManager;

use tddy_daemon_kernel::presenter_observer::SharedPresenterEventSink;

use std::sync::Arc;

use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;

use crate::config::DaemonConfig;

use std::path::PathBuf;

use tddy_rpc::Status;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Resolve the `tddy-tools` binary from this deployment's **toolchain** — the directory its
    /// tddy binaries are installed in ([`tddy_daemon_kernel::toolchain`]).
    ///
    /// This used to derive the path from `allowed_tools[0].path` by swapping the filename, which
    /// made two mistakes at once. It took the toolchain's location from a *UI menu* of
    /// `tddy-coder` builds, and it kept whatever form that entry had — relative in every dev
    /// config (`target/debug/tddy-coder`). The relative result was written verbatim into a
    /// session's `claude-mcp-config.json`, then spawned by an agent whose cwd is its session
    /// context dir, where no `target/` exists: the MCP server never started, and an agent that had
    /// every native tool withdrawn was left with no replacement it could reach. Nothing logged it.
    ///
    /// A path that does not exist is **refused here**, naming it, rather than written into a
    /// config for something downstream to fail on silently.
    /// The agent-facing tool socket, when this daemon is embedded in an application and therefore
    /// serves no HTTP listener for a co-located agent to POST to.
    ///
    /// Decided by whether the socket is *there* rather than by a host-kind flag threaded through
    /// the session layer: an embedded daemon binds it at startup (`tddy_daemon::runtime`), a binary
    /// one never does, and the path is derived from the same data dir on both sides — so the
    /// question "does this host serve that socket" is answered by the host itself, and a stale
    /// flag cannot disagree with reality.
    pub(crate) fn agent_tool_socket_for_embedded_host(&self) -> Option<String> {
        let path = tddy_daemon_kernel::agent_tool_socket_path(&self.tddy_data_dir);
        path.exists().then(|| path.to_string_lossy().into_owned())
    }

    pub(crate) fn resolve_tddy_tools_path(&self) -> Result<PathBuf, Status> {
        resolve_tddy_tools_path(&self.config)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: DaemonConfig,
        sessions_base_for_user: tddy_daemon_kernel::SessionsBaseResolver,
        tddy_data_dir: PathBuf,
        user_resolver: tddy_daemon_kernel::SessionUserResolver,
        spawn_client: Option<(spawn_worker::SpawnClient, i32)>,
        livekit_discovery: Option<LiveKitDiscoveryHandles>,
        presenter_event_sink: Option<SharedPresenterEventSink>,
        claude_cli_manager: Arc<CliSessionManager>,
    ) -> Self {
        let spawn_client = spawn_client.map(|(c, _pid)| Arc::new(c));
        let (eligible_daemon_source, common_room_livekit_room) = match livekit_discovery {
            Some(h) => (h.eligible_daemon_source, Some(h.common_room_livekit_room)),
            // No discovery: this machine is the only host, named the way its own configuration
            // names it — not by its hostname, which is merely the default when no
            // `daemon_instance_id` is set.
            None => (
                Arc::new(LocalOnlyEligibleDaemonSource::for_config(&config))
                    as Arc<dyn EligibleDaemonSource>,
                None,
            ),
        };
        let task_registry = claude_cli_manager.task_registry();
        let demo_vm_state = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let session_stdio = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let room_roster = room_roster_from_config(config.livekit.as_ref());
        // Built here rather than inline below because the roster store reads it: an entry's
        // `clone_state` is the state of the checkout serving it, and two stores would let a roster
        // report READY for a clone nobody built.
        let session_agent_clones =
            Arc::new(crate::session_agent_clone::SessionAgentCloneStore::new());
        let peer_routing = crate::peer_routing::PeerRouting::new(
            config.clone(),
            eligible_daemon_source,
            common_room_livekit_room,
        );
        Self {
            config,
            sessions_base_for_user,
            tddy_data_dir,
            user_resolver,
            spawn_client,
            peer_routing,
            presenter_event_sink,
            claude_cli_manager,
            sandbox_manager: Arc::new(
                tddy_daemon_sandbox::sandbox_session::SandboxSessionManager::new(),
            ),
            workspace_sandboxes: Arc::new(
                tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxRegistry::new(),
            ),
            workspace_sandbox_provisioner: Arc::new(
                tddy_daemon_sandbox::workspace_tool_sandbox::JailedWorkspaceSandboxProvisioner,
            ),
            task_registry,
            rpc_activity: crate::relay_idle::RpcActivity::default(),
            room_roster,
            roster_keepalive_interval: ROSTER_KEEPALIVE_INTERVAL,
            demo_vm_state,
            session_stdio,
            agent_activity_hub: Arc::new(tddy_daemon_kernel::AgentActivityHub::default()),
            session_agent_inference: Arc::new(
                crate::session_agent_inference::SessionAgentInferenceStore::new(),
            ),
            github_token_store: None,
            staging_base_dir: crate::session_attachment_staging::default_staging_base_dir(),
            session_rooms: Arc::new(tddy_daemon_livekit::session_room::SessionRoomRegistry::new()),
            model_registry: None,
            session_agent_rosters: Arc::new(
                crate::session_agent_roster::SessionAgentRosterStore::new(
                    Arc::clone(&session_agent_clones),
                    Arc::new(crate::session_agent_status::SessionAgentActivityStore::new()),
                ),
            ),
            session_agent_clones,
            hosted_agent_clones: Arc::new(crate::session_agent_clone::HostedAgentClones::new()),
            session_admissions: Arc::new(
                crate::session_admission_service::SessionAdmissionRegistry::new(),
            ),
            agent_conversations: Arc::new(tddy_session_agents::OpenAgentConversations::new()),
            // No bus until one is installed with `with_session_notification_bus`: the subscribers
            // are the daemon's to choose, and this service names none of them.
            session_notification_bus: None,
            sandbox_rpc_bridge: Arc::new(std::sync::OnceLock::new()),
            // Installed by the composition root once the handlers above this crate are built
            // from this host (`with_rpc_families`).
            rpc_families: None,
        }
    }

    /// Install the in-jail family-B relay once this service lives behind an `Arc` (see `runtime::build`).
    ///
    /// The handler holds a [`std::sync::Weak`] back to this host, never an `Arc`: the bridge it is
    /// stored in is a field of the host, so a strong reference would be a cycle that keeps the
    /// daemon — and therefore every jail in its `WorkspaceSandboxRegistry` — alive forever.
    pub fn install_sandbox_rpc_bridge(self: &Arc<Self>) {
        let handler: Arc<dyn tddy_sandbox_runner::HostRpcHandler> =
            Arc::new(super::DaemonRpcHandler {
                conn: Arc::downgrade(self),
            });
        let _ = self.sandbox_rpc_bridge.set(handler);
    }

    /// The host-side dispatch a sandboxed session's `SessionChannel` relays family B to.
    pub fn sandbox_rpc_handler(&self) -> Arc<dyn tddy_sandbox_runner::HostRpcHandler> {
        self.sandbox_rpc_bridge.get().cloned().expect(
            "sandbox RPC bridge not installed — runtime must call install_sandbox_rpc_bridge",
        )
    }

    /// Share this daemon's model registry (builder), so an assistant defined in it is listed by
    /// `ListAgents` as a selectable agent.
    pub fn with_model_registry(
        mut self,
        registry: Arc<tddy_model_registry::ModelRegistryStore>,
    ) -> Self {
        self.model_registry = Some(registry);
        self
    }

    /// The per-session admission registry — shared with the `SessionAdmissionService` served on
    /// the common room, so the attach path records the first admit, the RPC refreshes it, and the
    /// detach/session-delete paths revoke against the same set (PRD § "What attach does" step 3).
    pub fn session_admissions(
        &self,
    ) -> Arc<crate::session_admission_service::SessionAdmissionRegistry> {
        Arc::clone(&self.session_admissions)
    }

    /// Whether this daemon currently hosts the session room for `session_id` — the
    /// `SessionAdmissionService`'s `session_exists` check, exposed so a test fixture wiring the
    /// admission service against this daemon's registry can build the same checker `main.rs` does
    /// without reaching into private fields.
    pub fn hosts_session(&self, session_id: &str) -> bool {
        self.session_rooms.contains(session_id)
    }

    /// The shared session-room registry — so a test fixture wiring `SessionAdmissionService`
    /// against this daemon can build the `session_exists` checker `main.rs` builds over the same
    /// registry the connection service updates when it opens and closes a session room.
    pub fn session_rooms(&self) -> Arc<tddy_daemon_livekit::session_room::SessionRoomRegistry> {
        Arc::clone(&self.session_rooms)
    }

    /// The first admit — the facilitating daemon records the owning daemon in the admission registry
    /// and mints the scoped, short-TTL token it forwards along with the StartSession (PRD § "What
    /// attach does" step 3). The owning daemon joins `session-{session_id}` with this token and
    /// nothing else, then runs the re-admit loop against `AdmitOwningDaemon` before it expires.
    ///
    /// Returns `None` when this daemon cannot admit (LiveKit not configured), so a caller can skip
    /// the handshake and fall back to the owning daemon self-minting — never silently, but as a
    /// recorded deviation. `Some(token, url, room, ttl)` is what the caller forwards.
    pub(crate) fn mint_first_admission_token(
        &self,
        session_id: &str,
        owning_daemon_instance_id: &str,
    ) -> Option<(String, String, String, u64)> {
        use crate::livekit_peer_discovery::{
            daemon_rpc_identity, livekit_common_room_connect_strings,
        };
        use crate::session_admission_service::ADMISSION_TOKEN_TTL;
        use tddy_daemon_livekit::session_room::session_room_name;
        use tddy_livekit::TokenGenerator;

        let (_common_room, url, api_key, api_secret) =
            livekit_common_room_connect_strings(&self.config).ok()?;
        self.session_admissions
            .admit(session_id, owning_daemon_instance_id);
        let room = session_room_name(session_id);
        let identity = daemon_rpc_identity(owning_daemon_instance_id);
        let token = TokenGenerator::new(
            api_key,
            api_secret,
            room.clone(),
            identity,
            ADMISSION_TOKEN_TTL,
        )
        .generate()
        .ok()?;
        log::info!(
            "provision_agent_clone: minted first admission token for daemon \
             {owning_daemon_instance_id} to session {session_id} (room {room}, ttl={}s)",
            ADMISSION_TOKEN_TTL.as_secs()
        );
        Some((token, url, room, ADMISSION_TOKEN_TTL.as_secs()))
    }

    /// Share the daemon's session-room registry (builder) with everything else that opens or
    /// closes rooms — Telegram's Delete path holds the same `Arc`. Without it this service keeps
    /// the private registry it was constructed with, which is right for a test fixture and wrong
    /// for a daemon, where a room opened here has to be closable from there.
    pub fn with_session_rooms(
        mut self,
        rooms: Arc<tddy_daemon_livekit::session_room::SessionRoomRegistry>,
    ) -> Self {
        self.session_rooms = rooms;
        self
    }

    /// This daemon as the claimant of the clones a session's peer-owned agents read.
    pub(crate) fn seed_clone_claimant(&self) -> agent_roster::DaemonSeedCloneClaimant {
        agent_roster::DaemonSeedCloneClaimant {
            service: self.clone(),
        }
    }

    /// Substitute the pre-session attachment staging base (builder pattern) — lets a test point
    /// staging at a `TempDir` it owns and assert *where* staged bytes land, instead of sharing the
    /// process temp dir with every other test run.
    pub fn with_staging_base_dir(mut self, staging_base_dir: PathBuf) -> Self {
        self.set_staging_base_dir(staging_base_dir);
        self
    }

    pub(crate) fn set_staging_base_dir(&mut self, staging_base_dir: PathBuf) {
        self.staging_base_dir = staging_base_dir;
    }

    /// Act on the operator's own GitHub credential for PR-status reads (builder). The store is the
    /// one the auth service writes to at login; without it, PR status reports itself unavailable.
    pub fn with_github_token_store(
        mut self,
        store: Arc<dyn tddy_github::token_store::GitHubTokenStore>,
    ) -> Self {
        self.github_token_store = Some(store);
        self
    }

    /// Shared agent-activity hub, so the sandbox tool path can publish through the same channel the
    /// `StreamSessionActivity` subscribers read.
    pub fn agent_activity_hub(&self) -> Arc<tddy_daemon_kernel::AgentActivityHub> {
        Arc::clone(&self.agent_activity_hub)
    }

    /// Return the shared `TaskRegistry` so `main.rs` can pass it to other services.
    pub fn task_registry(&self) -> TaskRegistry {
        self.task_registry.clone()
    }

    /// Substitute the session-notification bus (builder pattern).
    ///
    /// [`Self::new`] installs none. `runtime.rs` installs the daemon's bus — Telegram's subscriber
    /// beside the `StreamSessionNotifications` relay — and a test installs one to record what a
    /// publish reached.
    pub fn with_session_notification_bus(
        mut self,
        bus: Arc<crate::session_notifications::SessionNotificationBus>,
    ) -> Self {
        self.session_notification_bus = Some(bus);
        self
    }

    /// Attach an idle-timeout tracker to this service (builder pattern).
    ///
    /// When set, every RPC handler calls `tracker.record_activity()` so the relay daemon does
    /// not self-terminate while a client is actively using the service.
    pub fn with_idle_tracker(
        mut self,
        tracker: Arc<crate::relay_idle::IdleTimeoutTracker>,
    ) -> Self {
        self.rpc_activity = crate::relay_idle::RpcActivity::on(tracker);
        self
    }

    /// Substitute the eligible-daemon source (builder pattern) — the live roster every host-facing
    /// handler joins against.
    ///
    /// Without this a test can only ever see the machine it runs on, so "a *remote* host is
    /// online" would be unprovable and the difference between `online` and `is_local` would go
    /// unpinned.
    pub fn with_eligible_daemon_source(
        mut self,
        eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    ) -> Self {
        self.set_eligible_daemon_source(eligible_daemon_source);
        self
    }

    pub(crate) fn set_eligible_daemon_source(
        &mut self,
        eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    ) {
        self.peer_routing
            .set_eligible_daemon_source(eligible_daemon_source);
    }

    /// Substitute what builds a sandboxed workspace session's jail (builder pattern) — lets a test
    /// prove which calls reach the jail, and what a start does when one cannot be built, without a
    /// kernel sandbox on the machine running it.
    pub fn with_workspace_sandbox_provisioner(
        mut self,
        provisioner: Arc<
            dyn tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxProvisioner,
        >,
    ) -> Self {
        self.set_workspace_sandbox_provisioner(provisioner);
        self
    }

    pub(crate) fn set_workspace_sandbox_provisioner(
        &mut self,
        provisioner: Arc<
            dyn tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxProvisioner,
        >,
    ) {
        self.workspace_sandbox_provisioner = provisioner;
    }

    /// Substitute the LiveKit rooms reader (builder pattern) — lets tests drive a scripted roster
    /// sequence in place of a live LiveKit server.
    pub fn with_room_roster(mut self, room_roster: Arc<dyn RoomRoster>) -> Self {
        self.room_roster = room_roster;
        self
    }

    /// Override the `StreamSessionAgents` keepalive cadence (builder pattern) — lets tests observe a
    /// re-sent roster without waiting the production eight seconds for it.
    pub fn with_roster_keepalive_interval(mut self, interval: Duration) -> Self {
        self.set_roster_keepalive_interval(interval);
        self
    }

    pub(crate) fn set_roster_keepalive_interval(&mut self, interval: Duration) {
        self.roster_keepalive_interval = interval;
    }

    /// The three things every routing decision here is made from: the configuration that says which
    /// instance id is *this* daemon's, the live roster of everyone else, and the resolver that turns
    /// a session token into the operator asking.
    ///
    /// Published together because they are only meaningful together — an instance id means nothing
    /// without the config, and a roster means nothing to a caller the resolver does not know.
    /// `runtime.rs` hands the same three to `tddy-host-service`, so "who can I route to" has one
    /// answer across both services; an acceptance suite waiting for a peer to appear asks the host
    /// service and must ask it against *this* roster, or it waits on a different one.
    #[must_use]
    pub fn routing_view(
        &self,
    ) -> (
        DaemonConfig,
        Arc<dyn EligibleDaemonSource>,
        tddy_daemon_kernel::SessionUserResolver,
    ) {
        (
            self.config.clone(),
            Arc::clone(self.peer_routing.eligible_daemon_source()),
            Arc::clone(&self.user_resolver),
        )
    }

    /// Record RPC activity in the idle-timeout tracker, if one is attached.
    pub(crate) fn record_rpc_activity(&self) {
        self.rpc_activity.record();
    }

    /// Start the presenter observer for a freshly spawned workflow session: the injected
    /// presenter-event sink (Telegram's surface) when this daemon has one, and — when it has a bus
    /// and can resolve `os_user`'s sessions directory to read the session's label from — the
    /// notification publish that raises its indicator.
    ///
    /// The two are independent. Gating the observer on Telegram would leave a workflow session's
    /// drawer dot permanently still on every daemon without a `telegram:` block, which is most of
    /// them; `spawn_presenter_observer_task` declines only when *neither* sink exists.
    pub(crate) fn maybe_spawn_presenter_observer(
        &self,
        os_user: &str,
        session_id: &str,
        grpc_port: u16,
    ) {
        let publishing = self.session_notification_bus.as_ref().and_then(|bus| {
            match crate::user_sessions_path::sessions_base_for_user(
                os_user,
                Some(&self.tddy_data_dir),
            ) {
                Some(sessions_base) => {
                    Some(crate::session_notifications::SessionNotificationPublishing {
                        bus: Arc::clone(bus),
                        sessions_base,
                        os_user: os_user.to_string(),
                    })
                }
                None => {
                    log::warn!(
                        "presenter observer for session {session_id}: no sessions base for os_user — its notifications will not be published"
                    );
                    None
                }
            }
        });
        crate::presenter_observer_task::spawn_presenter_observer_task(
            self.presenter_event_sink.clone(),
            publishing,
            session_id,
            grpc_port,
        );
    }
}

/// [`DaemonSessionHost::resolve_tddy_tools_path`] over the one field it reads, so a family handler
/// above this crate (`tddy-daemon-rpc`'s catalogue, probing an agent's models) resolves the binary
/// exactly the way session start does, without holding the host.
pub fn resolve_tddy_tools_path(config: &DaemonConfig) -> Result<PathBuf, Status> {
    config.toolchain().binary("tddy-tools").map_err(|e| {
        log::error!("resolve_tddy_tools_path: {e}");
        Status::failed_precondition(e.to_string())
    })
}
