//! `host.HostService` — the RPCs a browser asks a *machine* about.
//!
//! Every method here is addressed by `daemon_instance_id`, and five of them are routed before the
//! caller is even authenticated. That ordering is the whole design: a host's tooling, the prompts
//! it raises and the keys it can be given are facts about **one machine**, and a daemon that
//! answered them locally would hand back its own answer wearing the addressed host's name — a wrong
//! answer that reads exactly like a right one. Routing first also means a peer's user mapping is
//! never this daemon's to judge; the daemon that serves the call verifies the token itself.

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::host::{
    AddHostKeyRequest, AddHostKeyResponse, AnswerHostPromptRequest, AnswerHostPromptResponse,
    EligibleDaemonEntry, GetHostToolingRequest, GetHostToolingResponse, HostCpuStats,
    HostDiskStats, HostLoadStats, HostMemoryStats, HostService, HostStatsEvent, KnownHostEntry,
    ListEligibleDaemonsRequest, ListEligibleDaemonsResponse, ListHostKeyCandidatesRequest,
    ListHostKeyCandidatesResponse, ListKnownHostsRequest, ListKnownHostsResponse,
    ListSshConfigHostsRequest, ListSshConfigHostsResponse, StreamHostPromptsRequest,
    StreamHostStatsRequest,
};
use tddy_task::IdleTimeoutTracker;

use crate::host_keypair::HostKeypair;
use crate::host_prompts::HostPromptRegistry;
use crate::host_registry::HostRegistry;
use crate::host_stats::HostStats;
use crate::host_tooling::{HostTooling, HostToolingProbe};
use crate::host_tooling_cache::ToolingCache;
use crate::multi_host::EligibleDaemonSource;
use crate::ssh_agent_add::SshAgentKeyAdder;
use crate::stream::{MpscHostPromptStream, MpscHostStatsStream};

/// The proto coordinate this service answers on, and the one a forwarded call is re-addressed to.
///
/// Named once: a forward that named a different service than the one serving it would reach a
/// method that exists on the peer but is not this one, and the mismatch is invisible until a peer
/// answers the wrong question.
const SERVICE_NAME: &str = "host.HostService";

/// How long a tooling answer is served without probing again. Short enough that an operator who
/// fixes a host's `gh` login sees it on the next visit; long enough that a page load per operator
/// does not re-run `gh auth status` and a TCP connect per desktop protocol each time.
const HOST_TOOLING_CACHE_TTL: Duration = Duration::from_secs(30);

/// Default cadence for refreshing per-core CPU utilization on the host-stats sampling loop.
pub(crate) const HOST_CPU_INTERVAL: Duration = Duration::from_secs(5);

/// Default cadence for refreshing project-dir disk figures on the host-stats sampling loop.
pub(crate) const HOST_DISK_INTERVAL: Duration = Duration::from_secs(60);

/// Everything `host.HostService` answers from.
///
/// `Clone` is shallow and shared: every field is behind an `Arc` or is a value type, so a clone
/// talks to the same registry, prompt feed and agent. The server-streaming handlers need that —
/// they hand the work to a `tokio::spawn`ed producer which must own a `'static` service.
#[derive(Clone)]
pub struct HostServiceImpl {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    /// Who is reachable **right now**, which is what routing is decided against. Distinct from
    /// `host_registry`, which remembers every host ever seen.
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    /// The common room a forwarded call travels over. `None` is a daemon with no peers, and every
    /// forward refuses with `FailedPrecondition` rather than answering locally.
    common_room_livekit_room: Option<CommonRoom>,
    /// Bumped on every RPC that reaches this service, so relay mode's idle timer sees host traffic.
    idle_tracker: Option<Arc<IdleTimeoutTracker>>,
    /// Durable record of every host seen, behind `ListKnownHosts`.
    host_registry: Arc<dyn HostRegistry>,
    /// Probes what this host has installed and configured, behind `GetHostTooling`.
    host_tooling: Arc<dyn HostToolingProbe>,
    /// The tooling probe's answers, kept for [`HOST_TOOLING_CACHE_TTL`] and shared by concurrent
    /// callers, because every online Hosts-screen row asks on every visit. A probe that panicked is
    /// kept as its message for the same window.
    host_tooling_cache: Arc<ToolingCache<Result<HostTooling, String>>>,
    /// Questions this host is waiting on an operator to answer, behind `StreamHostPrompts` and
    /// `AnswerHostPrompt`. Shared across clones, so the answer arriving on one connection reaches
    /// the prompt raised on another.
    host_prompts: Arc<dyn HostPromptRegistry>,
    /// The keypair a prompt publishes so its answer can be encrypted end to end, and the only thing
    /// on this host able to read one back.
    host_keypair: Arc<dyn HostKeypair>,
    /// What puts an unlocked identity into this host's ssh-agent, behind `AddHostKey`. Injected
    /// because the add is the one step of the flow that touches the operator's real agent —
    /// everything before it (prompt, encryption, decrypt, unlock) runs for real in a test.
    ssh_agent_key_adder: Arc<dyn SshAgentKeyAdder>,
    /// How the private key an operator names is read — as **their** OS user, and only from inside
    /// that user's home. Injected for the same reason the agent is.
    host_user_files: Arc<dyn crate::host_private_key::HostUserFiles>,
    /// Live `StreamHostPrompts` pumps. Exists so a test can observe a **leaked** pump: the prompt
    /// stream is silent by design, so a pump that outlived its subscriber is indistinguishable from
    /// a correctly idle one from the outside.
    prompt_pumps: Arc<AtomicUsize>,
    /// Host machine stats (per-core CPU, memory, load, project-dir disk) for the Host Stats Footer.
    host_stats: Arc<dyn HostStats>,
    host_cpu_interval: Duration,
    host_disk_interval: Duration,
}

mod peer_forwards;

mod builders;

#[async_trait::async_trait]
impl HostService for HostServiceImpl {
    type StreamHostPromptsStream = MpscHostPromptStream;
    type StreamHostStatsStream = MpscHostStatsStream;

    async fn list_eligible_daemons(
        &self,
        request: Request<ListEligibleDaemonsRequest>,
    ) -> Result<Response<ListEligibleDaemonsResponse>, Status> {
        self.list_eligible_daemons_impl(request).await
    }

    /// Every host this daemon has a record of, live or not.
    ///
    /// `ListEligibleDaemons` answers "who can I route to now" and forgets a host the moment it
    /// leaves the room. This answers "what machines does tddy know about", which is what an operator
    /// staring at an unreachable host needs. Liveness is resolved per call, by intersecting the
    /// durable registry with the live roster — never read from disk.
    ///
    /// The join itself belongs to [`crate::host_registry::HostRegistry::known_hosts`], including
    /// the guarantee that the serving daemon always has a row: doing half of it here as well would
    /// leave the invariant provable only against a double that behaves like neither store.
    async fn list_known_hosts(
        &self,
        request: Request<ListKnownHostsRequest>,
    ) -> Result<Response<ListKnownHostsResponse>, Status> {
        self.list_known_hosts_impl(request).await
    }

    /// What a host has installed and configured.
    ///
    /// Addressed by `daemon_instance_id`; the existing peer routing relays it so the probes run on
    /// that host, as that host's OS user. Both facts are per-user — `git config` reads
    /// `$HOME/.gitconfig`, `gh auth status` reads `$HOME/.config/gh/hosts.yml` — so running them as
    /// the daemon's own user would answer for the wrong account.
    ///
    /// Routed **before** the caller is authenticated, like the roster RPCs and
    /// [`Self::resolve_stack_base`]: the token is verified by the daemon that serves the call, and
    /// a peer's user mapping is not this one's to judge. Authenticating first would refuse an
    /// operator whose GitHub user maps to an OS user on the host being probed but not on whichever
    /// host their browser happens to be talking to.
    async fn get_host_tooling(
        &self,
        request: Request<GetHostToolingRequest>,
    ) -> Result<Response<GetHostToolingResponse>, Status> {
        self.get_host_tooling_impl(request).await
    }

    /// Questions this host is waiting on an operator to answer.
    ///
    /// ⚠ The stream is silent almost all the time, so the sampling task **must** select on
    /// `tx.closed()` as well as breaking on a send error — see [`MpscHostPromptStream`]. The
    /// regression test for it is `packages/tddy-daemon/tests/stream_host_prompts_rpc.rs`.
    async fn stream_host_prompts(
        &self,
        request: Request<StreamHostPromptsRequest>,
    ) -> Result<Response<Self::StreamHostPromptsStream>, Status> {
        self.stream_host_prompts_impl(request).await
    }

    /// Submit the encrypted answer to a pending prompt.
    ///
    /// The request carries ciphertext only; the plaintext exists in this process for the duration of
    /// the unlock and is never persisted or logged.
    async fn answer_host_prompt(
        &self,
        request: Request<AnswerHostPromptRequest>,
    ) -> Result<Response<AnswerHostPromptResponse>, Status> {
        self.answer_host_prompt_impl(request).await
    }

    /// Load a private key into this host's ssh-agent.
    ///
    /// This is the call that raises a passphrase prompt: it issues one, waits for the answer to
    /// arrive on `AnswerHostPrompt`, decrypts it with this host's private key, unlocks the key at
    /// `subject`, hands the identity to the agent and drops the plaintext. It returns only once the
    /// add has succeeded or failed, so the browser learns the outcome from the call it started.
    ///
    /// Nothing about the answer — decrypted or not — is logged here at any level.
    async fn add_host_key(
        &self,
        request: Request<AddHostKeyRequest>,
    ) -> Result<Response<AddHostKeyResponse>, Status> {
        self.add_host_key_impl(request).await
    }

    /// The private keys this host's operator could load into their agent.
    ///
    /// What makes the key field on the Hosts row a picker instead of a typed path. Every path it
    /// offers is a path [`add_host_key`](Self::add_host_key) will accept: the same OS user, the
    /// same home, the same confinement — a listing whose choices the add then refused would be
    /// worse than no listing.
    ///
    /// Routed like the add for the same reason: the keys are files on one machine.
    ///
    /// Says nothing about what is on disk beyond the keys themselves — see
    /// [`crate::host_private_key::list_key_candidates`] for why an absent `~/.ssh`, an unreadable
    /// one and an empty one are one answer.
    async fn list_host_key_candidates(
        &self,
        request: Request<ListHostKeyCandidatesRequest>,
    ) -> Result<Response<ListHostKeyCandidatesResponse>, Status> {
        self.list_host_key_candidates_impl(request).await
    }

    /// Explicit OpenSSH `Host` aliases in this OS user's `~/.ssh/config`.
    ///
    /// Routed like [`Self::list_host_key_candidates`]: the file is on one machine. Honesty is the
    /// opposite of that listing: unreadable is FAILED, not empty.
    async fn list_ssh_config_hosts(
        &self,
        request: Request<ListSshConfigHostsRequest>,
    ) -> Result<Response<ListSshConfigHostsResponse>, Status> {
        self.list_ssh_config_hosts_impl(request).await
    }

    /// Stream host telemetry for the selected daemon. Authenticates `session_token`, then spawns a
    /// sampling task that emits one `HostStatsEvent` immediately (both CPU and disk), then refreshes
    /// CPU and disk on two independent cadences, pushing an event carrying the latest CPU and disk
    /// snapshot on each tick. The task ends when the receiver is dropped (client unsubscribe).
    async fn stream_host_stats(
        &self,
        request: Request<StreamHostStatsRequest>,
    ) -> Result<Response<Self::StreamHostStatsStream>, Status> {
        self.stream_host_stats_impl(request).await
    }
}

impl HostServiceImpl {
    /// Body of [`HostService::list_eligible_daemons`]; the trait method delegates here.
    pub(crate) async fn list_eligible_daemons_impl(
        &self,
        request: Request<ListEligibleDaemonsRequest>,
    ) -> Result<Response<ListEligibleDaemonsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = &self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let local_id = local_instance_id_for_config(&self.config);
        let daemons: Vec<EligibleDaemonEntry> = self
            .eligible_daemon_source
            .list_eligible_daemons()
            .into_iter()
            .map(|entry| EligibleDaemonEntry {
                instance_id: entry.instance_id.0.clone(),
                label: entry.label,
                is_local: entry.instance_id.0 == local_id,
            })
            .collect();

        Ok(Response::new(ListEligibleDaemonsResponse { daemons }))
    }

    /// Body of [`HostService::list_known_hosts`]; the trait method delegates here.
    pub(crate) async fn list_known_hosts_impl(
        &self,
        request: Request<ListKnownHostsRequest>,
    ) -> Result<Response<ListKnownHostsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = &self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        // The durable roster, not the routing one: the registry files a machine under the id that
        // survives its restarts, and intersecting those two id spaces would report the daemon
        // serving this very call as an offline stranger, next to a second row for itself.
        let live_roster = self.eligible_daemon_source.live_known_hosts();
        let local = crate::host_registry::local_host_sighting(&self.config);
        let now_unix_ms = crate::host_registry::now_unix_ms();
        let hosts: Vec<KnownHostEntry> = self
            .host_registry
            .known_hosts(&live_roster, &local, now_unix_ms)
            .into_iter()
            .map(|view| KnownHostEntry {
                instance_id: view.host.instance_id,
                label: view.host.label,
                online: view.online,
                first_seen_unix_ms: view.host.first_seen_unix_ms,
                last_seen_unix_ms: view.host.last_seen_unix_ms,
                repos_base_path: view.host.repos_base_path,
                max_attachment_bytes: view.host.max_attachment_bytes,
                is_local: view.is_local,
            })
            .collect();

        Ok(Response::new(ListKnownHostsResponse { hosts }))
    }

    /// Body of [`HostService::get_host_tooling`]; the trait method delegates here.
    pub(crate) async fn get_host_tooling_impl(
        &self,
        request: Request<GetHostToolingRequest>,
    ) -> Result<Response<GetHostToolingResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route before probing. Answered locally, a question about another host would come back
        // with this daemon's own git identity under that host's name — a wrong answer that reads
        // exactly like a right one.
        if let Some(answered) = self
            .rpc_served_by_peer("GetHostTooling", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = &self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let local_instance_id = local_instance_id_for_config(&self.config);
        let tooling = self
            .cached_tooling(&local_instance_id, os_user, &req.github_hostname)
            .await
            .map_err(Status::internal)?;

        Ok(Response::new(GetHostToolingResponse {
            daemon_instance_id: local_instance_id,
            git: Some(crate::host_messages::git_identity_message(&tooling.git)),
            github_cli: Some(crate::host_messages::github_cli_message(
                &tooling.github_cli,
            )),
            ssh_agent: Some(crate::host_messages::ssh_agent_message(&tooling.ssh_agent)),
            remote_desktop: tooling
                .remote_desktop
                .iter()
                .map(crate::host_messages::host_remote_desktop_message)
                .collect(),
        }))
    }

    /// Body of [`HostService::stream_host_stats`]; the trait method delegates here.
    pub(crate) async fn stream_host_stats_impl(
        &self,
        request: Request<StreamHostStatsRequest>,
    ) -> Result<Response<MpscHostStatsStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<HostStatsEvent>();
        let host_stats = Arc::clone(&self.host_stats);
        let cpu_interval = self.host_cpu_interval;
        let disk_interval = self.host_disk_interval;

        tokio::spawn(async move {
            let read_cpu = |hs: &Arc<dyn HostStats>| HostCpuStats {
                per_core_percent: hs.cpu_per_core_percent(),
                logical_cores: hs.logical_cores(),
            };
            // Memory and load ride the fast tick with CPU: they move on the same timescale, and a
            // third timer would cost a builder parameter for no user-visible gain.
            let read_memory = |hs: &Arc<dyn HostStats>| {
                let usage = hs.memory();
                HostMemoryStats {
                    available_bytes: usage.available_bytes,
                    total_bytes: usage.total_bytes,
                }
            };
            // `None` stays `None` all the way to the wire — a host that cannot report a load average
            // must not be indistinguishable from an idle one.
            let read_load = |hs: &Arc<dyn HostStats>| {
                hs.load_average().map(|avg| HostLoadStats {
                    one_minute: avg.one_minute,
                    five_minutes: avg.five_minutes,
                    fifteen_minutes: avg.fifteen_minutes,
                })
            };
            let read_disk = |hs: &Arc<dyn HostStats>| {
                let usage = hs.disk_for_project_dir();
                HostDiskStats {
                    available_bytes: usage.available_bytes,
                    total_bytes: usage.total_bytes,
                    project_dir: usage.project_dir,
                }
            };

            // Immediate emit: read both snapshots once so the footer populates on connect.
            let mut cpu = read_cpu(&host_stats);
            let mut disk = read_disk(&host_stats);
            let mut memory = read_memory(&host_stats);
            let mut load = read_load(&host_stats);
            if tx
                .send(HostStatsEvent {
                    cpu: Some(cpu.clone()),
                    disk: Some(disk.clone()),
                    memory: Some(memory),
                    load,
                })
                .is_err()
            {
                return;
            }

            // Two independent timers: the first tick of each fires after one full period (not
            // immediately), so a tick provably reflects a fresh read of only that metric.
            let now = tokio::time::Instant::now();
            let mut cpu_tick = tokio::time::interval_at(now + cpu_interval, cpu_interval);
            let mut disk_tick = tokio::time::interval_at(now + disk_interval, disk_interval);

            loop {
                tokio::select! {
                    _ = cpu_tick.tick() => {
                        cpu = read_cpu(&host_stats);
                        memory = read_memory(&host_stats);
                        load = read_load(&host_stats);
                    }
                    _ = disk_tick.tick() => {
                        disk = read_disk(&host_stats);
                    }
                }
                if tx
                    .send(HostStatsEvent {
                        cpu: Some(cpu.clone()),
                        disk: Some(disk.clone()),
                        memory: Some(memory),
                        load,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Ok(Response::new(MpscHostStatsStream { rx }))
    }
}

mod prompts;

mod host_keys;
