use tddy_rpc::Status;

use crate::{connection_service::{activity_hub, agent_roster, service_util}, livekit_rooms_stream::RoomRoster, spawn_worker, worktrees};

use std::time::Duration;

use tddy_task::TaskRegistry;

use super::ROSTER_KEEPALIVE_INTERVAL;

use super::LIVEKIT_ROOMS_POLL_INTERVAL;

use super::HOST_DISK_INTERVAL;

use super::HOST_CPU_INTERVAL;

use crate::livekit_rooms_stream::room_roster_from_config;

use super::resolve_default_project_dir;

use crate::host_stats::SysinfoHostStats;

use crate::host_stats::HostStats;

use crate::ssh_agent_add::SshAgentKeyAdder;

use crate::host_keypair::HostKeypair;

use crate::host_prompts::HostPromptRegistry;

use crate::host_tooling::SubprocessHostToolingProbe;

use crate::host_tooling::HostToolingProbe;

use crate::host_registry::FileHostRegistry;

use crate::host_registry::HostRegistry;

use crate::worktrees::WorktreeSizeCalculator;

use crate::worktrees::WorktreeStatsCache;

use crate::multi_host::EligibleDaemonSource;

use crate::multi_host::LocalOnlyEligibleDaemonSource;

use crate::cli_session_manager::CliSessionManager;

use crate::telegram_session_subscriber::TelegramDaemonHooks;

use std::sync::Arc;

use crate::livekit_peer_discovery::LiveKitDiscoveryHandles;

use crate::config::DaemonConfig;

use std::path::Path;

use std::path::PathBuf;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    /// Resolve the `tddy-tools` binary as a sibling of the configured tool (`tddy-coder`) path, so
    /// an installed deployment and a dev `target/debug` tree both find the co-located binary. Falls
    /// back to a bare `tddy-tools` (PATH lookup) when the tool path has no directory component.
    pub(crate) fn resolve_tddy_tools_path(&self) -> PathBuf {
        let base = self.config.default_tool_path();
        let base = Path::new(&base);
        match base.parent().filter(|p| !p.as_os_str().is_empty()) {
            Some(dir) => dir.join("tddy-tools"),
            None => PathBuf::from("tddy-tools"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: DaemonConfig,
        sessions_base_for_user: service_util::SessionsBaseResolver,
        tddy_data_dir: PathBuf,
        user_resolver: service_util::SessionUserResolver,
        spawn_client: Option<(spawn_worker::SpawnClient, i32)>,
        livekit_discovery: Option<LiveKitDiscoveryHandles>,
        telegram: Option<Arc<TelegramDaemonHooks>>,
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
        let worktree_stats_cache = Arc::new(WorktreeStatsCache::new(
            worktrees::projects_stats_cache_root(&tddy_data_dir),
        ));
        // Daemon-global cap of 2 concurrent size walks; shares the stats cache root so a fresh
        // calculator serves persisted sizes without re-walking.
        let worktree_size_calculator = Arc::new(WorktreeSizeCalculator::new(
            worktrees::projects_stats_cache_root(&tddy_data_dir),
            2,
        ));
        let task_registry = claude_cli_manager.task_registry();
        let demo_vm_state = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let session_stdio = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let host_registry: Arc<dyn HostRegistry> = Arc::new(FileHostRegistry::new(
            crate::host_registry::host_registry_dir(&tddy_data_dir),
        ));
        // Given this daemon's own configuration, not a default one: the desktop block's
        // bridge-availability flag is an existence check on the path `screen_sharing` resolves to,
        // and an operator who set that path explicitly must have it checked.
        let host_tooling: Arc<dyn HostToolingProbe> =
            Arc::new(SubprocessHostToolingProbe::for_config(&config));
        let host_prompts: Arc<dyn HostPromptRegistry> =
            Arc::new(crate::host_prompts::InMemoryHostPromptRegistry::new());
        // Alongside the host registry, and generated on first use rather than here: a host whose
        // operator never adds a key never pays for an RSA keygen.
        let host_keypair: Arc<dyn HostKeypair> =
            Arc::new(crate::host_keypair::FileHostKeypair::new(
                crate::host_registry::host_registry_dir(&tddy_data_dir),
            ));
        let prompt_pumps = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let ssh_agent_key_adder: Arc<dyn SshAgentKeyAdder> =
            Arc::new(crate::ssh_agent_add::WireProtocolAgentKeyAdder);
        let host_user_files: Arc<dyn crate::host_private_key::HostUserFiles> =
            Arc::new(crate::host_private_key::SpawnedHostUserFiles);
        let host_stats: Arc<dyn HostStats> =
            Arc::new(SysinfoHostStats::new(resolve_default_project_dir(&config)));
        let room_roster = room_roster_from_config(config.livekit.as_ref());
        // Built here rather than inline below because the roster store reads it: an entry's
        // `clone_state` is the state of the checkout serving it, and two stores would let a roster
        // report READY for a clone nobody built.
        let session_agent_clones =
            Arc::new(crate::session_agent_clone::SessionAgentCloneStore::new());
        // A service given Telegram hooks and no explicit bus still notifies Telegram, because the
        // notification bus is now the only path from `ReportSessionStatus` to a chat. Without this
        // the hooks would be inert until a caller happened to install a bus, and "Telegram is
        // configured" would stop meaning "Telegram is notified". `main.rs` overrides it with a bus
        // carrying the notification stream alongside Telegram.
        let session_notification_bus = telegram.as_ref().map(|hooks| {
            Arc::new(
                crate::session_notifications::SessionNotificationBus::new()
                    .with_subscriber(Arc::new(
                    crate::session_notification_subscribers::TelegramNotificationSubscriber::new(
                        Arc::clone(hooks),
                    ),
                )),
            )
        });
        Self {
            config,
            sessions_base_for_user,
            tddy_data_dir,
            user_resolver,
            spawn_client,
            eligible_daemon_source,
            host_registry,
            host_tooling,
            host_prompts,
            host_keypair,
            ssh_agent_key_adder,
            host_user_files,
            prompt_pumps,
            common_room_livekit_room,
            telegram,
            worktree_stats_cache,
            worktree_size_calculator,
            claude_cli_manager,
            sandbox_manager: Arc::new(crate::sandbox_session::SandboxSessionManager::new()),
            workspace_sandboxes: Arc::new(
                crate::workspace_tool_sandbox::WorkspaceSandboxRegistry::new(),
            ),
            workspace_sandbox_provisioner: Arc::new(
                crate::workspace_tool_sandbox::JailedWorkspaceSandboxProvisioner,
            ),
            task_registry,
            idle_tracker: None,
            host_stats,
            host_cpu_interval: HOST_CPU_INTERVAL,
            host_disk_interval: HOST_DISK_INTERVAL,
            room_roster,
            room_poll_interval: LIVEKIT_ROOMS_POLL_INTERVAL,
            roster_keepalive_interval: ROSTER_KEEPALIVE_INTERVAL,
            demo_vm_state,
            session_stdio,
            agent_activity_hub: Arc::new(activity_hub::AgentActivityHub::default()),
            session_agent_inference: Arc::new(
                crate::session_agent_inference::SessionAgentInferenceStore::new(),
            ),
            github_token_store: None,
            staging_base_dir: crate::session_attachment_staging::default_staging_base_dir(),
            session_rooms: Arc::new(crate::session_room::SessionRoomRegistry::new()),
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
            agent_conversations: Arc::new(
                tokio::sync::Mutex::new(std::collections::HashMap::new()),
            ),
            session_notification_bus,
            self_handle: Arc::new(std::sync::OnceLock::new()),
        }
    }

    /// Record the `Weak` to the top-level `Arc<ConnectionServiceImpl>` so a `&self` method can
    /// recover the `Arc` via [`Self::self_arc`]. Called once, right after `Arc::new`, in `runtime.rs`.
    /// Shared across `Clone`s (the field is an `Arc<OnceLock<…>>`), so a clone tonic holds still
    /// sees the same handle. Idempotent: a second call is a no-op, which is what tests want when
    /// they re-construct a service in the same process.
    pub fn set_self_handle(&self, handle: std::sync::Weak<ConnectionServiceImpl>) {
        let _ = self.self_handle.set(handle);
    }

    /// Recover the `Arc<ConnectionServiceImpl>` this `&self` belongs to. Panics if
    /// [`Self::set_self_handle`] was never called — which is a wiring bug, not a runtime condition:
    /// the daemon's `main.rs` sets it at startup, and tests that do not exercise the sandbox-IPC
    /// RPC bridge never call this.
    pub fn self_arc(&self) -> Arc<ConnectionServiceImpl> {
        self.self_handle
            .get()
            .and_then(|weak| weak.upgrade())
            .expect("ConnectionServiceImpl::self_arc called before set_self_handle")
    }

    /// Share this daemon's model registry (builder), so an assistant defined in it is listed by
    /// `ListAgents` as a selectable agent.
    pub fn with_model_registry(
        mut self,
        registry: Arc<crate::model_registry::ModelRegistryStore>,
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
    pub fn session_rooms(&self) -> Arc<crate::session_room::SessionRoomRegistry> {
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
        use crate::session_room::session_room_name;
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
        rooms: Arc<crate::session_room::SessionRoomRegistry>,
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
        self.staging_base_dir = staging_base_dir;
        self
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
    pub fn agent_activity_hub(&self) -> Arc<activity_hub::AgentActivityHub> {
        Arc::clone(&self.agent_activity_hub)
    }

    /// Return the shared `TaskRegistry` so `main.rs` can pass it to other services.
    pub fn task_registry(&self) -> TaskRegistry {
        self.task_registry.clone()
    }

    /// Substitute the session-notification bus (builder pattern).
    ///
    /// Replaces the Telegram-only bus [`Self::new`] builds from the hooks it was given, which is
    /// what `main.rs` does to add the `StreamSessionNotifications` relay beside Telegram, and what
    /// a test does to record what a publish reached.
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
        self.idle_tracker = Some(tracker);
        self
    }

    /// How many `StreamHostPrompts` pumps are currently running.
    ///
    /// A pump must not outlive its subscriber: the stream is silent by nature, so without the
    /// `tokio::select!` on `tx.closed()` the task parks forever on a prompt that never comes,
    /// leaking one per subscription for the life of the daemon.
    #[must_use]
    pub fn pending_prompt_pump_count(&self) -> usize {
        self.prompt_pumps.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Substitute the host prompt registry (builder pattern).
    ///
    /// Lets a test hold the same registry the handlers use, so it can see the prompt `AddHostKey`
    /// raised and answer it — the operator's half of a flow that otherwise has no other end.
    pub fn with_host_prompts(mut self, host_prompts: Arc<dyn HostPromptRegistry>) -> Self {
        self.host_prompts = host_prompts;
        self
    }

    /// Substitute the host keypair (builder pattern).
    ///
    /// A test encrypts its answer against the published half exactly as the browser does, so the
    /// real RSA-OAEP decrypt runs rather than being stood in for.
    pub fn with_host_keypair(mut self, host_keypair: Arc<dyn HostKeypair>) -> Self {
        self.host_keypair = host_keypair;
        self
    }

    /// Substitute what an unlocked identity is handed to (builder pattern).
    ///
    /// The only seam in the add-key flow that replaces real behaviour: a test must not load a key
    /// into the agent of whoever is running the suite.
    pub fn with_ssh_agent_key_adder(mut self, adder: Arc<dyn SshAgentKeyAdder>) -> Self {
        self.ssh_agent_key_adder = adder;
        self
    }

    /// Substitute how an operator's files are reached (builder pattern).
    ///
    /// Stands in for impersonating an OS user, which a test cannot do. What it must **not** stand
    /// in for is the confinement: the home directory it reports is the one the real check runs
    /// against.
    pub fn with_host_user_files(
        mut self,
        files: Arc<dyn crate::host_private_key::HostUserFiles>,
    ) -> Self {
        self.host_user_files = files;
        self
    }

    /// Substitute the host tooling probe (builder pattern) — lets tests state what a host has
    /// installed instead of depending on whatever is installed on the machine running the suite.
    pub fn with_host_tooling(mut self, host_tooling: Arc<dyn HostToolingProbe>) -> Self {
        self.host_tooling = host_tooling;
        self
    }

    /// Substitute the known-host registry (builder pattern) — lets tests inject a deterministic,
    /// in-memory registry in place of the file-backed one, and drive `online` from a stub roster.
    pub fn with_host_registry(mut self, host_registry: Arc<dyn HostRegistry>) -> Self {
        self.host_registry = host_registry;
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
        self.eligible_daemon_source = eligible_daemon_source;
        self
    }

    /// Substitute the host machine stats provider (builder pattern) — lets tests inject a
    /// deterministic fake in place of the live `sysinfo`-backed provider.
    pub fn with_host_stats(mut self, host_stats: Arc<dyn HostStats>) -> Self {
        self.host_stats = host_stats;
        self
    }

    /// Substitute the per-worktree disk-size calculator (builder pattern) — lets tests inject a
    /// deterministic, instant sizer via [`WorktreeSizeCalculator::with_sizer`] in place of the live
    /// directory walk.
    pub fn with_worktree_size_calculator(
        mut self,
        calculator: Arc<WorktreeSizeCalculator>,
    ) -> Self {
        self.worktree_size_calculator = calculator;
        self
    }

    /// Override the `StreamHostStats` sampling cadence (builder pattern) — lets tests inject tiny
    /// intervals so cadence-driven refresh can be asserted deterministically without real-time waits.
    pub fn with_host_stats_intervals(mut self, cpu: Duration, disk: Duration) -> Self {
        self.host_cpu_interval = cpu;
        self.host_disk_interval = disk;
        self
    }

    /// Substitute what builds a sandboxed workspace session's jail (builder pattern) — lets a test
    /// prove which calls reach the jail, and what a start does when one cannot be built, without a
    /// kernel sandbox on the machine running it.
    pub fn with_workspace_sandbox_provisioner(
        mut self,
        provisioner: Arc<dyn crate::workspace_tool_sandbox::WorkspaceSandboxProvisioner>,
    ) -> Self {
        self.workspace_sandbox_provisioner = provisioner;
        self
    }

    /// Substitute the LiveKit rooms reader (builder pattern) — lets tests drive a scripted roster
    /// sequence in place of a live LiveKit server.
    pub fn with_room_roster(mut self, room_roster: Arc<dyn RoomRoster>) -> Self {
        self.room_roster = room_roster;
        self
    }

    /// Override the `StreamLiveKitRooms` poll cadence (builder pattern) — lets tests observe a
    /// change event without waiting the production three seconds for it.
    pub fn with_room_poll_interval(mut self, interval: Duration) -> Self {
        self.room_poll_interval = interval;
        self
    }

    /// Override the `StreamSessionAgents` keepalive cadence (builder pattern) — lets tests observe a
    /// re-sent roster without waiting the production eight seconds for it.
    pub fn with_roster_keepalive_interval(mut self, interval: Duration) -> Self {
        self.roster_keepalive_interval = interval;
        self
    }

    /// Record RPC activity in the idle-timeout tracker, if one is attached.
    pub(crate) fn record_rpc_activity(&self) {
        if let Some(ref tracker) = self.idle_tracker {
            tracker.record_activity();
        }
    }

    /// Resolves the caller's per-user sessions base from a `session_token`, rejecting an invalid
    /// token before any filesystem access. Shared by the session-uploads RPCs (list/delete), which
    /// address files under `{sessions_base}/sessions/{session_id}/uploads/`.
    pub(crate) fn uploads_sessions_base(&self, session_token: &str) -> Result<PathBuf, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))
    }

    /// Start the presenter observer for a freshly spawned workflow session: Telegram's surface when
    /// this daemon has one, and — when it has a bus and can resolve `os_user`'s sessions directory
    /// to read the session's label from — the notification publish that raises its indicator.
    ///
    /// The two are independent. Gating the observer on Telegram would leave a workflow session's
    /// drawer dot permanently still on every daemon without a `telegram:` block, which is most of
    /// them; `spawn_presenter_observer_task` declines only when *neither* sink exists.
    pub(crate) fn maybe_spawn_presenter_observer(&self, os_user: &str, session_id: &str, grpc_port: u16) {
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
        crate::telegram_session_subscriber::spawn_presenter_observer_task(
            self.telegram.clone(),
            publishing,
            session_id,
            grpc_port,
        );
    }

}
