//! `host.HostService` — the eight RPCs a browser asks a *machine* about.
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

use livekit::Room;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::peer_forwarding::{
    classify_peer_route, forward_server_stream_to_peer, forward_to_peer, PeerRoute,
};
use tddy_daemon_kernel::SessionUserResolver;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::host::{
    AddHostKeyOutcome, AddHostKeyRequest, AddHostKeyResponse, AnswerHostPromptRequest,
    AnswerHostPromptResponse, EligibleDaemonEntry, GetHostToolingRequest, GetHostToolingResponse,
    HostCpuStats, HostDiskStats, HostKeyCandidate, HostLoadStats, HostMemoryStats, HostPromptEvent,
    HostService, HostStatsEvent, KnownHostEntry, ListEligibleDaemonsRequest,
    ListEligibleDaemonsResponse, ListHostKeyCandidatesRequest, ListHostKeyCandidatesResponse,
    ListKnownHostsRequest, ListKnownHostsResponse, StreamHostPromptsRequest,
    StreamHostStatsRequest,
};
use tddy_task::IdleTimeoutTracker;

use crate::host_keypair::HostKeypair;
use crate::host_prompts::{answer_before_expiry, HostPromptRegistry, PromptKind};
use crate::host_registry::{FileHostRegistry, HostRegistry};
use crate::host_stats::{HostStats, SysinfoHostStats};
use crate::host_tooling::{HostToolingProbe, SubprocessHostToolingProbe};
use crate::multi_host::{EligibleDaemonSource, LocalOnlyEligibleDaemonSource};
use crate::ssh_agent_add::SshAgentKeyAdder;
use crate::stream::{MpscHostPromptStream, MpscHostStatsStream};

/// The proto coordinate this service answers on, and the one a forwarded call is re-addressed to.
///
/// Named once: a forward that named a different service than the one serving it would reach a
/// method that exists on the peer but is not this one, and the mismatch is invisible until a peer
/// answers the wrong question.
const SERVICE_NAME: &str = "host.HostService";

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
    common_room_livekit_room: Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>>,
    /// Bumped on every RPC that reaches this service, so relay mode's idle timer sees host traffic.
    idle_tracker: Option<Arc<IdleTimeoutTracker>>,
    /// Durable record of every host seen, behind `ListKnownHosts`.
    host_registry: Arc<dyn HostRegistry>,
    /// Probes what this host has installed and configured, behind `GetHostTooling`.
    host_tooling: Arc<dyn HostToolingProbe>,
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

    /// The common-room connection a forwarded call travels over.
    #[must_use]
    pub fn with_common_room(mut self, room: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>) -> Self {
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

    /// How many `StreamHostPrompts` pumps are running right now.
    ///
    /// A leaked pump is invisible from the outside — the feed is silent by design — so this is what
    /// a test asserts against after dropping a subscriber.
    #[must_use]
    pub fn pending_prompt_pump_count(&self) -> usize {
        self.prompt_pumps.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn record_rpc_activity(&self) {
        if let Some(ref tracker) = self.idle_tracker {
            tracker.record_activity();
        }
    }

    fn eligible_instance_ids(&self) -> Vec<String> {
        self.eligible_daemon_source
            .list_eligible_daemons()
            .into_iter()
            .map(|e| e.instance_id.0)
            .collect()
    }

    /// Decide whether `requested_daemon` names this daemon or another one.
    ///
    /// An empty id is this daemon's to serve: that is the protocol's other spelling for "the daemon
    /// this call arrived on". An id that names nobody is `InvalidArgument` — the caller addressed a
    /// host that cannot serve the call, which is a bad request rather than a deployment that is not
    /// ready yet.
    fn classify_addressed_daemon_route(
        &self,
        rpc_name: &str,
        requested_daemon: &str,
    ) -> Result<PeerRoute, Status> {
        let requested_daemon = requested_daemon.trim();
        if requested_daemon.is_empty() {
            return Ok(PeerRoute::Local);
        }
        let route = classify_peer_route(
            &local_instance_id_for_config(&self.config),
            requested_daemon,
            &self.eligible_instance_ids(),
        )
        .map_err(|msg| {
            log::info!("{rpc_name}: rejected daemon routing: {msg}");
            Status::invalid_argument(msg)
        })?;
        if let PeerRoute::Forward { peer_instance_id } = &route {
            log::info!(
                "{rpc_name}: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
        }
        Ok(route)
    }

    fn common_room_slot(
        &self,
        rpc_name: &str,
    ) -> Result<&Arc<tokio::sync::RwLock<Option<Arc<Room>>>>, Status> {
        self.common_room_livekit_room.as_ref().ok_or_else(|| {
            Status::failed_precondition(format!(
                "cannot forward {rpc_name}: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)"
            ))
        })
    }

    /// Serve a unary RPC on the daemon its `daemon_instance_id` names, when that is not this one.
    ///
    /// `Ok(None)` means the call is this daemon's own to serve — an empty id, or this daemon's id.
    /// `rpc_name` is the proto method name, so a forwarded call lands on the same handler there.
    async fn rpc_served_by_peer<Req, Resp>(
        &self,
        rpc_name: &str,
        requested_daemon: &str,
        req: &Req,
    ) -> Result<Option<Resp>, Status>
    where
        Req: prost::Message,
        Resp: prost::Message + Default,
    {
        let PeerRoute::Forward { peer_instance_id } =
            self.classify_addressed_daemon_route(rpc_name, requested_daemon)?
        else {
            return Ok(None);
        };
        let slot = self.common_room_slot(rpc_name)?;
        let answered = forward_to_peer(
            slot,
            &peer_instance_id,
            SERVICE_NAME,
            rpc_name,
            req.encode_to_vec(),
        )
        .await?;
        Resp::decode(answered.as_slice())
            .map(Some)
            .map_err(|e| Status::internal(format!("decode {rpc_name} response from peer: {e}")))
    }

    /// [`Self::rpc_served_by_peer`] for a **server-streaming** RPC: the peer's frames are relayed
    /// one by one, and a stream that stops without its end-of-stream marker terminates as an error
    /// rather than as a short feed the caller would take for the whole one.
    async fn stream_served_by_peer<Req, Frame>(
        &self,
        rpc_name: &str,
        requested_daemon: &str,
        req: &Req,
    ) -> Result<Option<tokio::sync::mpsc::UnboundedReceiver<Result<Frame, Status>>>, Status>
    where
        Req: prost::Message,
        Frame: prost::Message + Default + Send + 'static,
    {
        let PeerRoute::Forward { peer_instance_id } =
            self.classify_addressed_daemon_route(rpc_name, requested_daemon)?
        else {
            return Ok(None);
        };
        let slot = self.common_room_slot(rpc_name)?;
        let decoding = rpc_name.to_string();
        forward_server_stream_to_peer(
            slot,
            &peer_instance_id,
            SERVICE_NAME,
            rpc_name,
            req.encode_to_vec(),
            move |bytes| {
                Frame::decode(bytes.as_slice()).map_err(|e| {
                    Status::internal(format!("decode {decoding} frame from peer: {e}"))
                })
            },
        )
        .await
        .map(Some)
    }
}

#[async_trait::async_trait]
impl HostService for HostServiceImpl {
    type StreamHostPromptsStream = MpscHostPromptStream;
    type StreamHostStatsStream = MpscHostStatsStream;

    async fn list_eligible_daemons(
        &self,
        request: Request<ListEligibleDaemonsRequest>,
    ) -> Result<Response<ListEligibleDaemonsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
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
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
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
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        // Both probes shell out and wait, so they run on the blocking pool rather than parking a
        // runtime worker for however long `gh` takes to reach the network.
        let probe = Arc::clone(&self.host_tooling);
        let probed_user = os_user.to_string();
        let tooling = tokio::task::spawn_blocking(move || probe.probe(&probed_user))
            .await
            .map_err(|e| Status::internal(format!("host tooling probe panicked: {e}")))?;

        Ok(Response::new(GetHostToolingResponse {
            daemon_instance_id: local_instance_id_for_config(&self.config),
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

    /// Questions this host is waiting on an operator to answer.
    ///
    /// ⚠ The stream is silent almost all the time, so the sampling task **must** select on
    /// `tx.closed()` as well as breaking on a send error — see [`MpscHostPromptStream`]. The
    /// regression test for it is `packages/tddy-daemon/tests/stream_host_prompts_rpc.rs`.
    async fn stream_host_prompts(
        &self,
        request: Request<StreamHostPromptsRequest>,
    ) -> Result<Response<Self::StreamHostPromptsStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Routed before anything else, as `GetHostTooling` routes: a prompt is raised by, and
        // answerable on, exactly one host, and a browser that asked one host what it is waiting on
        // would take this daemon's own questions for that host's.
        if let Some(rx) = self
            .stream_served_by_peer("StreamHostPrompts", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(MpscHostPromptStream { rx }));
        }

        // Kept, not discarded: this is the identity the feed is filtered by. A prompt names a
        // private-key path one operator typed, and it is theirs alone to see and to answer.
        let subscriber = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<HostPromptEvent, Status>>();
        let prompts = Arc::clone(&self.host_prompts);
        let keypair = Arc::clone(&self.host_keypair);
        let daemon_instance_id = local_instance_id_for_config(&self.config);
        let counted = crate::host_prompt_stream::PumpCount::running(Arc::clone(&self.prompt_pumps));

        tokio::spawn(async move {
            // Moved into the task rather than dropped at the end of it, so the count falls when the
            // pump actually stops — including if it panics.
            let _counted = counted;
            crate::host_prompt_stream::pump_host_prompts(
                prompts,
                keypair,
                daemon_instance_id,
                subscriber,
                tx,
            )
            .await;
        });

        Ok(Response::new(MpscHostPromptStream { rx }))
    }

    /// Submit the encrypted answer to a pending prompt.
    ///
    /// The request carries ciphertext only; the plaintext exists in this process for the duration of
    /// the unlock and is never persisted or logged.
    async fn answer_host_prompt(
        &self,
        request: Request<AnswerHostPromptRequest>,
    ) -> Result<Response<AnswerHostPromptResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Routed before the prompt is looked up, and before the caller is authenticated — the same
        // order `GetHostTooling` uses, and for the same reason: the prompt this answers exists on
        // the host that raised it, and resolved here it belongs to nothing. Answered locally, an
        // operator's passphrase would be spent on a refusal from the wrong machine.
        if let Some(answered) = self
            .rpc_served_by_peer("AnswerHostPrompt", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let answered_by = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        // A refusal is a `false` on the response, not a `Status` error: an expired or replayed
        // prompt is an ordinary outcome of an operator taking their time, and the browser has to
        // tell the operator which of the three it was.
        //
        // Accepting the ciphertext is what hands it over: the registry passes it straight down the
        // handoff `AddHostKey` is waiting on, which decrypts it, unlocks the key, adds the identity
        // and drops the plaintext. An answer nobody is waiting for is still recorded as this
        // prompt's one answer, and its ciphertext is dropped rather than kept.
        //
        // Nothing about the payload is logged here at any level, deliberately: a passphrase must
        // never reach a log, and the cheapest way to keep that true is for this handler to have
        // nothing to say about what it was given.
        //
        // The answering session's own identity decides which prompt it may answer: a prompt raised
        // by somebody else is refused exactly as an id that was never issued is, and is left
        // unspent for the operator it belongs to.
        let answered = self.host_prompts.answer(
            &req.prompt_id,
            &answered_by,
            req.encrypted_answer,
            crate::host_registry::now_unix_ms(),
        );
        Ok(Response::new(match answered {
            Ok(()) => AnswerHostPromptResponse {
                accepted: true,
                rejection_reason: String::new(),
            },
            Err(rejection) => AnswerHostPromptResponse {
                accepted: false,
                rejection_reason: crate::host_messages::rejection_reason(&rejection),
            },
        }))
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
        self.record_rpc_activity();
        let req = request.into_inner();

        // Routed before the prompt is raised. `daemon_instance_id` names the host whose agent the
        // key is loaded into, and answered locally this call loads it into the agent of whichever
        // daemon the browser happened to be talking to — a private key in the wrong machine's
        // agent, which no later request can take back.
        if let Some(answered) = self
            .rpc_served_by_peer("AddHostKey", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        // The agent the key lands in is the one belonging to this host's OS user, resolved exactly
        // as `GetHostTooling` resolves the user whose agent it *reads*: a session that may look at
        // an agent's keys is the session that may add one to it.
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?
            .to_string();

        // Stamped with the GitHub user that raised it, which is what makes it *this* operator's
        // question: nobody else is shown it, and nobody else can spend its one answer.
        let prompt = self.host_prompts.issue(
            &github_user,
            PromptKind::SshKeyPassphrase,
            &req.subject,
            crate::host_registry::now_unix_ms(),
        );
        // Claimed immediately after issuing, because issuing is what puts the prompt on the feed:
        // an operator whose browser answers at once must find a handoff already waiting for them.
        let waiting = self.host_prompts.awaited_answer(&prompt.prompt_id);
        let answer = match waiting {
            Some(handoff) => answer_before_expiry(handoff, &prompt).await,
            // The registry forgot the prompt between issuing it and being asked for its handoff,
            // which for the operator is indistinguishable from one that ran out of time.
            None => None,
        };
        let Some(encrypted_answer) = answer else {
            return Ok(Response::new(crate::host_messages::add_key_failed(
                AddHostKeyOutcome::PromptExpired,
                "nobody answered the passphrase prompt before it expired".to_string(),
            )));
        };

        // Everything that follows blocks — an RSA decrypt, a bcrypt-pbkdf unlock and a socket
        // conversation with the agent — so it runs on the blocking pool rather than parking a
        // runtime worker. It also puts the whole life of the plaintext inside one closure, which
        // ends when the closure returns.
        let keypair = Arc::clone(&self.host_keypair);
        let adder = Arc::clone(&self.ssh_agent_key_adder);
        let files = Arc::clone(&self.host_user_files);
        let subject = req.subject.clone();
        let added = tokio::task::spawn_blocking(move || {
            crate::host_messages::unlock_and_add(
                keypair.as_ref(),
                adder.as_ref(),
                files.as_ref(),
                &os_user,
                &subject,
                &encrypted_answer,
            )
        })
        .await
        // The panic's own message is deliberately not repeated: a panic raised inside the unlock is
        // the one string in this flow that could carry key material with it.
        .map_err(|_| Status::internal("adding this key to the agent did not complete"))?;

        // The outcome only — never the reason, and never anything derived from the answer.
        log::debug!(
            "AddHostKey: {} -> {}",
            req.subject,
            AddHostKeyOutcome::try_from(added.outcome)
                .unwrap_or(AddHostKeyOutcome::Unspecified)
                .as_str_name()
        );
        Ok(Response::new(added))
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
        self.record_rpc_activity();
        let req = request.into_inner();

        // Routed first, for the reason the add is: these paths are files on one machine, and a
        // listing answered locally shows the browser this daemon's keys as though they were the
        // addressed host's — after which the path it picks names nothing over there.
        if let Some(answered) = self
            .rpc_served_by_peer("ListHostKeyCandidates", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        // The same mapping `add_host_key` resolves the read through, so what an operator is offered
        // and what they may then add are the keys of one OS user — their own.
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?
            .to_string();

        // Off the runtime worker: the listing runs a child process per user-privileged step, the
        // way every other per-user read in this daemon does.
        let files = Arc::clone(&self.host_user_files);
        let candidates = tokio::task::spawn_blocking(move || {
            crate::host_private_key::list_key_candidates(files.as_ref(), &os_user)
        })
        .await
        .map_err(|_| Status::internal("listing the keys on this host did not complete"))?;

        log::debug!("ListHostKeyCandidates: {} offered", candidates.len());
        Ok(Response::new(ListHostKeyCandidatesResponse {
            candidates: candidates
                .into_iter()
                .map(|candidate| HostKeyCandidate {
                    path: candidate.path.display().to_string(),
                    key_type: candidate.key_type,
                    fingerprint: candidate.fingerprint,
                })
                .collect(),
        }))
    }

    /// Stream host telemetry for the selected daemon. Authenticates `session_token`, then spawns a
    /// sampling task that emits one `HostStatsEvent` immediately (both CPU and disk), then refreshes
    /// CPU and disk on two independent cadences, pushing an event carrying the latest CPU and disk
    /// snapshot on each tick. The task ends when the receiver is dropped (client unsubscribe).
    async fn stream_host_stats(
        &self,
        request: Request<StreamHostStatsRequest>,
    ) -> Result<Response<Self::StreamHostStatsStream>, Status> {
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
