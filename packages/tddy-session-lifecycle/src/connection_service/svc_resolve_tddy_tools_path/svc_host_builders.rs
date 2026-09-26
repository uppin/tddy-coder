use super::DaemonSessionHost;

use std::time::Duration;

use tddy_daemon_livekit::{
    livekit_peer_discovery::LiveKitDiscoveryHandles,
    livekit_rooms_stream::{room_roster_from_config, RoomRoster},
};

use tddy_host_service::multi_host::LocalOnlyEligibleDaemonSource;
use tddy_spawn::spawn_worker;
use tddy_task::TaskRegistry;

use crate::{
    connection_service::{agent_roster, ROSTER_KEEPALIVE_INTERVAL},
    multi_host::EligibleDaemonSource,
    CliSessionManager,
};

use std::sync::Arc;

use tddy_daemon_kernel::presenter_observer::SharedPresenterEventSink;

use std::path::PathBuf;

use crate::config::DaemonConfig;

impl DaemonSessionHost {
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
            jail_relaunch: Arc::new(super::super::jail_relaunch::JailRelaunch::default()),
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
            credential_vaults: None,
            session_tokens: None,
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
            Arc::new(super::super::DaemonRpcHandler {
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
        self.debug_assert_rpc_families_not_installed("with_model_registry");
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

    /// Act on the operator's own GitHub credential for PR-status reads (builder). The vaults are
    /// the ones the auth service opens at login and reopens at refresh; without them, PR status
    /// reports itself unavailable.
    pub fn with_credential_vaults(mut self, vaults: Arc<tddy_daemon_auth::SessionVaults>) -> Self {
        self.debug_assert_rpc_families_not_installed("with_credential_vaults");
        self.credential_vaults = Some(vaults);
        self
    }

    /// Sign agents' credentials with this daemon's key and verify callers' through its key
    /// directory (builder). Pass the very value the daemon's auth entries were built with, so the
    /// credentials minted here are ones every gate on the fleet already trusts.
    pub fn with_session_tokens(mut self, tokens: tddy_daemon_auth::SessionTokens) -> Self {
        self.session_tokens = Some(tokens);
        self
    }

    /// The signer and verifier an agent's own credential is minted with, or the refusal a daemon
    /// that signs nothing gives.
    pub(crate) fn session_tokens(
        &self,
    ) -> Result<&tddy_daemon_auth::SessionTokens, tddy_rpc::Status> {
        self.session_tokens.as_ref().ok_or_else(|| {
            tddy_rpc::Status::failed_precondition(
                "this daemon signs no session tokens, so an agent's tool calls could not be \
                 authenticated: configure `github:` — which gives the daemon its signing \
                 identity — and retry",
            )
        })
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
        self.debug_assert_rpc_families_not_installed("with_idle_tracker");
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
        self.debug_assert_rpc_families_not_installed("with_eligible_daemon_source");
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
}

mod first_admission_token;

mod rpc_activity;

mod presenter_observer_spawn;
