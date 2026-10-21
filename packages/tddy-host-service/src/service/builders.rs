use super::HostServiceImpl;

use crate::{
    host_registry::{FileHostRegistry, HostRegistry},
    host_tooling::SubprocessHostToolingProbe,
    host_tooling_cache::ToolingKey,
};

use crate::host_tooling::HostTooling;

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;

use std::time::Duration;

use crate::host_stats::HostStats;

use crate::ssh_agent_add::SshAgentKeyAdder;

use crate::host_keypair::HostKeypair;

use crate::host_tooling::HostToolingProbe;

use crate::host_prompts::HostPromptRegistry;

use tddy_task::IdleTimeoutTracker;

use tddy_daemon_kernel::peer_forwarding::CommonRoom;

use crate::multi_host::EligibleDaemonSource;

use super::HOST_DISK_INTERVAL;

use super::HOST_CPU_INTERVAL;

use crate::host_stats::SysinfoHostStats;

use std::sync::atomic::AtomicUsize;

use crate::host_tooling_cache::SystemClock;

use super::HOST_TOOLING_CACHE_TTL;

use crate::host_tooling_cache::ToolingCache;

use crate::multi_host::LocalOnlyEligibleDaemonSource;

use std::sync::Arc;

use tddy_daemon_kernel::SessionUserResolver;

use tddy_daemon_kernel::config::DaemonConfig;

impl HostServiceImpl {
    /// A service wired to this host's own real registry, probe, prompt feed, agent and stats.
    ///
    /// Every one of those is replaceable through a `with_*` builder; the defaults are here so the
    /// daemon's wiring layer states only what it overrides, and so a caller cannot forget one and
    /// get a service that silently answers about nothing.
    pub fn new(
        config: DaemonConfig,
        tddy_data_dir: &std::path::Path,
        user_resolver: SessionUserResolver,
    ) -> Self {
        let registry_dir = crate::host_registry::host_registry_dir(tddy_data_dir);
        Self {
            // This machine, named the way its own configuration names it — not by its hostname,
            // which is merely the default when no `daemon_instance_id` is set.
            eligible_daemon_source: Arc::new(LocalOnlyEligibleDaemonSource::for_config(&config)),
            common_room_livekit_room: None,
            idle_tracker: None,
            host_registry: Arc::new(FileHostRegistry::new(registry_dir.clone())),
            // Given this daemon's own configuration, not a default one: the desktop block's
            // bridge-availability flag is an existence check on the path `screen_sharing` resolves
            // to, and an operator who set that path explicitly must have it checked.
            host_tooling: Arc::new(SubprocessHostToolingProbe::for_config(&config)),
            host_tooling_cache: Arc::new(ToolingCache::new(
                HOST_TOOLING_CACHE_TTL,
                Arc::new(SystemClock),
            )),
            host_prompts: Arc::new(crate::host_prompts::InMemoryHostPromptRegistry::new()),
            // Alongside the host registry, and generated on first use rather than here: a host
            // whose operator never adds a key never pays for an RSA keygen.
            host_keypair: Arc::new(crate::host_keypair::FileHostKeypair::new(registry_dir)),
            ssh_agent_key_adder: Arc::new(crate::ssh_agent_add::WireProtocolAgentKeyAdder),
            host_user_files: Arc::new(crate::host_private_key::SpawnedHostUserFiles),
            prompt_pumps: Arc::new(AtomicUsize::new(0)),
            host_stats: Arc::new(SysinfoHostStats::new(
                crate::host_stats::resolve_default_project_dir(&config),
            )),
            host_cpu_interval: HOST_CPU_INTERVAL,
            host_disk_interval: HOST_DISK_INTERVAL,
            user_resolver,
            config,
        }
    }

    /// Route against the live common-room roster instead of this machine alone.
    #[must_use]
    pub fn with_eligible_daemon_source(mut self, source: Arc<dyn EligibleDaemonSource>) -> Self {
        self.eligible_daemon_source = source;
        self
    }

    /// The common room a forwarded call travels through. It carries its own deadline, so build it
    /// with the same config this service was given.
    #[must_use]
    pub fn with_common_room(mut self, room: CommonRoom) -> Self {
        self.common_room_livekit_room = Some(room);
        self
    }

    /// Count host RPCs as activity for relay mode's idle timer.
    #[must_use]
    pub fn with_idle_tracker(mut self, tracker: Arc<IdleTimeoutTracker>) -> Self {
        self.idle_tracker = Some(tracker);
        self
    }

    /// Share **one** registry with everything else that raises or answers a host prompt.
    ///
    /// A prompt raised on one instance and answered on another is a question nobody can answer, so
    /// the screen-sharing service that raises one for a desktop password is handed this same one.
    #[must_use]
    pub fn with_host_prompts(mut self, host_prompts: Arc<dyn HostPromptRegistry>) -> Self {
        self.host_prompts = host_prompts;
        self
    }

    #[must_use]
    pub fn with_host_registry(mut self, host_registry: Arc<dyn HostRegistry>) -> Self {
        self.host_registry = host_registry;
        self
    }

    #[must_use]
    pub fn with_host_tooling(mut self, host_tooling: Arc<dyn HostToolingProbe>) -> Self {
        self.host_tooling = host_tooling;
        self
    }

    #[must_use]
    pub fn with_host_keypair(mut self, host_keypair: Arc<dyn HostKeypair>) -> Self {
        self.host_keypair = host_keypair;
        self
    }

    #[must_use]
    pub fn with_ssh_agent_key_adder(mut self, adder: Arc<dyn SshAgentKeyAdder>) -> Self {
        self.ssh_agent_key_adder = adder;
        self
    }

    #[must_use]
    pub fn with_host_user_files(
        mut self,
        files: Arc<dyn crate::host_private_key::HostUserFiles>,
    ) -> Self {
        self.host_user_files = files;
        self
    }

    #[must_use]
    pub fn with_host_stats(mut self, host_stats: Arc<dyn HostStats>) -> Self {
        self.host_stats = host_stats;
        self
    }

    /// Sample faster than production does, so a test observes a second tick without waiting for it.
    #[must_use]
    pub fn with_host_stats_intervals(mut self, cpu: Duration, disk: Duration) -> Self {
        self.host_cpu_interval = cpu;
        self.host_disk_interval = disk;
        self
    }

    /// The host keypair this service publishes on its prompts — the public half a browser encrypts
    /// an answer under, and the only private half able to read one back.
    #[must_use]
    pub fn host_keypair(&self) -> Arc<dyn HostKeypair> {
        Arc::clone(&self.host_keypair)
    }

    /// The prompt registry this service streams and answers.
    #[must_use]
    pub fn host_prompts(&self) -> Arc<dyn HostPromptRegistry> {
        Arc::clone(&self.host_prompts)
    }

    /// The routing id this service answers to — what a caller puts in `daemon_instance_id` to
    /// address *this* host rather than a peer.
    ///
    /// Already part of the contract: `GetHostTooling` stamps its response with it, so a caller that
    /// has to name this host has to be able to ask for the same string the response carries.
    #[must_use]
    pub fn local_instance_id(&self) -> String {
        local_instance_id_for_config(&self.config)
    }

    /// `os_user`'s tooling on this host, served from the cache when a probe for the same
    /// (host, OS user, GitHub host) ran within the TTL, or shared with one already running.
    ///
    /// An empty `github_hostname` is the protocol's spelling for "none named". The probe shells
    /// out and waits, so it runs on the blocking pool rather than parking a runtime worker for
    /// however long `gh` takes to reach the network.
    pub(crate) async fn cached_tooling(
        &self,
        local_instance_id: &str,
        os_user: &str,
        github_hostname: &str,
    ) -> Result<HostTooling, String> {
        let github_hostname = Some(github_hostname.to_string()).filter(|h| !h.is_empty());
        let key = ToolingKey::new(local_instance_id, os_user, github_hostname.clone());
        let probe = Arc::clone(&self.host_tooling);
        let probed_user = os_user.to_string();
        self.host_tooling_cache
            .get_or_probe(key, move || async move {
                tokio::task::spawn_blocking(move || {
                    probe.probe(&probed_user, github_hostname.as_deref())
                })
                .await
                .map_err(|e| format!("host tooling probe panicked: {e}"))
            })
            .await
    }

    /// How many `StreamHostPrompts` pumps are running right now.
    ///
    /// A leaked pump is invisible from the outside — the feed is silent by design — so this is what
    /// a test asserts against after dropping a subscriber.
    #[must_use]
    pub fn pending_prompt_pump_count(&self) -> usize {
        self.prompt_pumps.load(std::sync::atomic::Ordering::SeqCst)
    }
}
