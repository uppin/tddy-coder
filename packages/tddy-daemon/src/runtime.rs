//! The daemon as a library: assemble its service roster and lifecycle handles once, for whichever
//! process is hosting it.
//!
//! `tddy-daemon`'s `main` is one host. A desktop application that embeds the daemon in its own
//! process is another, and it needs the same roster: a session started over the webview's IPC
//! bridge must reach the same services, in the same registrations, as one started against the
//! binary. Two bootstrap paths would drift, so there is one, and [`RuntimeHost`] carries the only
//! differences — who serves HTTP, and who may adopt a systemd-activated socket.
//!
//! [`build`] is assembly: it derives every service from configuration and returns the handles.
//! Nothing that listens, dials or runs forever is started there — the web listener belongs to the
//! host, and the rest is handed back as [`RuntimeTasks`] for the host to start when it is ready.
//! That is what makes the roster reproducible: two hosts (or two calls) assemble the same services
//! without competing for a port, a socket path or a LiveKit identity.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tddy_service::proto::daemon_config::DaemonConfigServiceServer;
use teloxide::prelude::Bot;
use tokio::sync::Mutex;

use crate::common_room_supervisor::{
    cloned_entries, CommonRoomSupervisorTask, CommonRoomTarget, DaemonCommonRoomConnector,
    PeerDiscoveryHandles, SupervisedCommonRoom,
};
use crate::config::DaemonConfig;
use crate::daemon_config_service::{CommonRoomSupervisor, DaemonConfigServiceImpl};
use crate::telegram_notifier::TelegramSender;

/// Which process is hosting this runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeHost {
    /// The `tddy-daemon` binary: serves the web bundle and RPC over HTTP, and may adopt a socket
    /// passed by systemd.
    Binary,
    /// An application that embeds the daemon in its own process: no HTTP listener, no socket
    /// activation. Its UI reaches the roster over the host application's own transport.
    Embedded,
}

/// How to build the runtime.
#[derive(Clone)]
pub struct RuntimeOptions {
    pub host: RuntimeHost,
    /// Relay mode: how long the daemon may sit idle before it shuts itself down. `Some` wires an
    /// idle tracker into the roster and yields [`DaemonRuntime::relay_shutdown`]; `None` is a
    /// daemon that runs until it is stopped.
    pub relay_idle_timeout: Option<Duration>,
    /// Where a GitHub sign-in comes back to, when the host serves the callback itself rather than
    /// relying on a web server. `None` leaves the configured value alone.
    pub oauth_redirect_uri: Option<String>,
    /// The spawn worker this host forked *before* starting a runtime, with its pid.
    ///
    /// It cannot be forked here: `fork` from a multi-threaded process can deadlock, and [`build`]
    /// already runs on the host's async runtime. `None` means this daemon spawns nothing itself —
    /// either because `tddy-supervisor` does it (see [`crate::supervisor_client`]) or because the
    /// host has no worker to offer.
    pub spawn_client: Option<(crate::spawn_worker::SpawnClient, i32)>,
    /// The YAML file this daemon was loaded from, which `daemon_config.DaemonConfigService` writes
    /// an accepted update back to. `None` — a host that configured the daemon in code — makes every
    /// update a refusal, because there is nowhere to persist one.
    pub config_path: Option<PathBuf>,
}

impl std::fmt::Debug for RuntimeOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeOptions")
            .field("host", &self.host)
            .field("relay_idle_timeout", &self.relay_idle_timeout)
            .field(
                "spawn_worker",
                &self.spawn_client.as_ref().map(|(_, pid)| pid),
            )
            .field("config_path", &self.config_path)
            .finish()
    }
}

impl RuntimeOptions {
    /// Options for the `tddy-daemon` binary.
    pub fn for_binary() -> Self {
        Self {
            host: RuntimeHost::Binary,
            oauth_redirect_uri: None,
            relay_idle_timeout: None,
            spawn_client: None,
            config_path: None,
        }
    }

    /// Options for a process that embeds the daemon.
    pub fn for_embedded() -> Self {
        Self {
            host: RuntimeHost::Embedded,
            oauth_redirect_uri: None,
            relay_idle_timeout: None,
            spawn_client: None,
            config_path: None,
        }
    }

    /// Run in relay mode, shutting down after `timeout` of idleness.
    pub fn with_relay_idle_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.relay_idle_timeout = timeout;
        self
    }

    /// Where a GitHub sign-in should come back to, when the host serves the callback itself.
    ///
    /// Applied to the auth services only — never to the configuration this runtime keeps, which is
    /// what a settings update persists. A host that overrode the stored value would write an
    /// address the operator never chose into their file, and break sign-in for anyone else using it.
    pub fn with_oauth_redirect(mut self, redirect_uri: Option<String>) -> Self {
        self.oauth_redirect_uri = redirect_uri;
        self
    }

    /// Name the YAML file this daemon was loaded from, so an accepted settings update can be
    /// written back to it.
    pub fn with_config_path(mut self, config_path: Option<PathBuf>) -> Self {
        self.config_path = config_path;
        self
    }

    /// Hand over the spawn worker this host forked before its runtime started.
    pub fn with_spawn_worker(
        mut self,
        spawn_client: Option<(crate::spawn_worker::SpawnClient, i32)>,
    ) -> Self {
        self.spawn_client = spawn_client;
        self
    }
}

/// An assembled daemon: every service it hosts, plus the configuration it was built from.
pub struct DaemonRuntime {
    /// The RPC roster, ready to hand to `MultiRpcService` — over HTTP, over LiveKit, or over a
    /// webview IPC bridge.
    pub entries: Vec<tddy_rpc::ServiceEntry>,
    /// The configuration this runtime was built from, after environment overrides.
    pub config: DaemonConfig,
    /// The claude-cli sessions this daemon owns. The host kills them when it is shutting down —
    /// they outlive the RPC surface otherwise.
    pub cli_sessions: Arc<crate::cli_session_manager::CliSessionManager>,
    /// The Telegram chat that gets the "started"/"stopped" messages, when a bot is configured.
    pub lifecycle_telegram: Option<(DaemonConfig, Arc<dyn TelegramSender + Send + Sync>)>,
    /// Relay mode: fires once the idle timeout expires, for a server that shuts down gracefully.
    /// The monitor that fires it is started by [`RuntimeTasks::spawn`].
    pub relay_shutdown: Option<tokio::sync::oneshot::Receiver<()>>,
    /// Everything this runtime needs running but has not started: see [`RuntimeTasks`].
    pub tasks: RuntimeTasks,
}

impl DaemonRuntime {
    /// The names of the services this runtime hosts, in registration order.
    pub fn service_names(&self) -> Vec<&str> {
        self.entries.iter().map(|entry| entry.name).collect()
    }
}

/// The listeners, dialers and background loops an assembled runtime needs, none of them started.
///
/// [`build`] returns them instead of spawning them so that assembling a runtime binds nothing and
/// joins nothing; the host calls [`spawn`](RuntimeTasks::spawn) once it is ready to accept traffic.
pub struct RuntimeTasks {
    common_room: Option<CommonRoomSupervisorTask>,
    oauth_loopback_tunnel: Option<OauthLoopbackTunnel>,
    local_socket: Option<LocalSocketTransport>,
    lsp_idle_reaper: Option<tddy_lsp::LspRegistry>,
    relay_idle_monitor: Option<(
        Arc<crate::relay_idle::IdleTimeoutTracker>,
        tokio::sync::oneshot::Sender<()>,
    )>,
    telegram_inbound: Option<TelegramInbound>,
}

/// The OAuth loopback TCP proxy, which follows the common room across a reconnect rather than
/// belonging to any one connection — see [`crate::livekit_peer_discovery::spawn_oauth_loopback_tunnel`].
struct OauthLoopbackTunnel {
    config: Arc<DaemonConfig>,
    room_slot: Arc<tokio::sync::RwLock<Option<Arc<livekit::Room>>>>,
}

/// The local Unix-domain-socket `ConnectionService` transport (SO_PEERCRED peer-trust plus
/// `MintLocalToken`). Only the binary host serves it — the socket path is per-daemon, and a
/// systemd-activated listener is addressed to the binary's pid.
struct LocalSocketTransport {
    socket_path: PathBuf,
    adapter: crate::connection_tonic_adapter::ConnectionServiceTonicAdapter<
        crate::connection_service::ConnectionServiceImpl,
    >,
}

/// The Telegram bot's inbound dispatcher: the buttons an operator presses in a chat.
struct TelegramInbound {
    bot: Bot,
    harness: Arc<
        Mutex<
            crate::telegram_session_control::TelegramSessionControlHarness<
                crate::telegram_notifier::TeloxideSender,
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
            if let Some(handle) = crate::livekit_peer_discovery::spawn_oauth_loopback_tunnel(
                &tunnel.config,
                tunnel.room_slot,
            ) {
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
                    local_socket.adapter,
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

        if let Some(inbound) = self.telegram_inbound {
            handles.push(tokio::spawn(async move {
                if let Err(e) =
                    crate::telegram_bot::run_telegram_bot(inbound.bot, inbound.harness).await
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

/// Apply environment variable overrides to config (e.g. from .env loaded by web-dev).
///
/// Also sets `codex_oauth_loopback_proxy_eligible` from `TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE`
/// when present.
pub fn apply_env_overrides(config: &mut DaemonConfig) {
    if let Some(v) = env_var("LIVEKIT_PUBLIC_URL") {
        if let Some(ref mut lk) = config.livekit {
            lk.public_url = Some(v.clone());
            lk.url = Some(v);
        }
    }
    if let Some(v) = env_var("LIVEKIT_URL") {
        if let Some(ref mut lk) = config.livekit {
            lk.url = Some(v);
        }
    }
    if let Some(v) = env_var("LIVEKIT_API_KEY") {
        if let Some(ref mut lk) = config.livekit {
            lk.api_key = Some(v);
        }
    }
    if let Some(v) = env_var("LIVEKIT_API_SECRET") {
        if let Some(ref mut lk) = config.livekit {
            lk.api_secret = Some(v);
        }
    }
    if let Some(v) = env_var("WEB_HOST") {
        config.listen.web_host = Some(v);
    }
    if let Some(v) = env_var("WEB_PUBLIC_URL") {
        let base = v.trim_end_matches('/');
        if let Some(ref mut g) = config.github {
            g.redirect_uri = Some(format!("{}/auth/callback", base));
        }
    }
    if let Some(v) = env_var("GITHUB_CLIENT_ID") {
        if let Some(ref mut g) = config.github {
            g.client_id = Some(v);
        }
    }
    if let Some(v) = env_var("GITHUB_CLIENT_SECRET") {
        if let Some(ref mut g) = config.github {
            g.client_secret = Some(v);
        }
    }
    if let Some(v) = env_var("GITHUB_REDIRECT_URI") {
        if let Some(ref mut g) = config.github {
            g.redirect_uri = Some(v);
        }
    }
    if let Some(v) = env_var("TDDY_DAEMON_INSTANCE_ID") {
        config.daemon_instance_id = Some(v);
    }
    config.apply_oauth_loopback_proxy_env_override();
    config.apply_telegram_env_overrides();
    config.apply_timing_env_overrides();
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

/// The tddy home data directory: config is the single source of truth.
/// Where this daemon keeps sessions and state, given what the configuration says and what the
/// environment overrides it with.
///
/// `TDDY_DATA_DIR` wins: it is how a development run is kept out of the `~/.tddy` an installed app
/// owns, and how one machine can host two daemons that share a configuration file without sharing
/// their sessions. Taken as a parameter rather than read here so the rule is testable without
/// process-wide state.
fn data_dir_from(config: &DaemonConfig, env_override: Option<String>) -> Option<PathBuf> {
    env_override
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| config.tddy_data_dir.clone())
}

fn tddy_data_dir_for(config: &DaemonConfig) -> PathBuf {
    data_dir_from(config, env_var("TDDY_DATA_DIR"))
        .or_else(tddy_core::output::default_tddy_data_dir)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".tddy")
        })
}

/// Assemble the daemon described by `config` for the host named in `options`.
pub async fn build(
    mut config: DaemonConfig,
    options: RuntimeOptions,
) -> anyhow::Result<DaemonRuntime> {
    // Apply env overrides (e.g. from .env loaded by web-dev) before anything reads the config.
    apply_env_overrides(&mut config);

    let tddy_data_dir = tddy_data_dir_for(&config);

    let web_host = config
        .listen
        .web_host
        .clone()
        .unwrap_or_else(|| "0.0.0.0".to_string());
    // Only used to synthesise the OAuth redirect URI a login comes back to. Absent, no login could
    // complete, so there is nothing to assemble.
    let web_port = config
        .listen
        .web_port
        .ok_or_else(|| anyhow::anyhow!("config.listen.web_port is required"))?;

    // The override reaches the auth services and stops there: `config` is what the settings service
    // persists, and a host-chosen callback address written into an operator's file would be a value
    // they never set — and would send a browser sign-in to loopback on some other machine.
    let auth_config = match &options.oauth_redirect_uri {
        Some(redirect_uri) => {
            let mut overridden = config.clone();
            if let Some(github) = overridden.github.as_mut() {
                github.redirect_uri = Some(redirect_uri.clone());
            }
            overridden
        }
        None => config.clone(),
    };
    let auth_result = crate::auth::build_auth_entries(&auth_config, web_host.as_str(), web_port)?;
    let mut rpc_entries = auth_result.entries;

    // The room-JWT mint the web UI joins rooms through. Gated on the same session token as every
    // other daemon RPC — anything that can reach `/rpc` can call it, and a JWT it minted is
    // admission to a LiveKit room. See `crate::auth::build_token_service_entry`.
    if let Some(entry) =
        crate::auth::build_token_service_entry(&config, auth_result.user_resolver.as_ref())
    {
        rpc_entries.push(entry);
    }

    // The handle the config service reconfigures the LiveKit connection through. Built before the
    // roster because the roster holds it; the connection it supervises is assembled at the end,
    // once there is a roster to serve on the room.
    let common_room = Arc::new(SupervisedCommonRoom::new(config.livekit.as_ref()));
    // The same rule that admits a caller to every other gated daemon RPC — this one reads and
    // writes the daemon's own LiveKit credentials.
    let config_service_authenticator = crate::auth::session_token_authenticator(
        auth_result.user_resolver.as_ref(),
        DaemonConfigServiceServer::<DaemonConfigServiceImpl>::NAME,
    );

    // Create one shared ClaudeCliSessionManager — injected into both the Telegram spawn path and
    // ConnectionServiceImpl so that Telegram-launched sessions are attachable via the terminal RPCs.
    let shared_claude_cli_manager = Arc::new(crate::cli_session_manager::CliSessionManager::new());

    // One registry of hosted session rooms for the whole daemon: `StartSession` opens rooms in it
    // and both deletion paths — the `DeleteSession` RPC and Telegram's Delete button — close them
    // there. Two registries would mean a room only one of them could ever stop hosting.
    let shared_session_rooms = Arc::new(crate::session_room::SessionRoomRegistry::new());

    let telegram = build_telegram(
        &config,
        &tddy_data_dir,
        &options,
        &shared_claude_cli_manager,
        &shared_session_rooms,
    );
    let telegram_hooks = telegram.hooks;
    let lifecycle_telegram = telegram_hooks.as_ref().map(|h| {
        (
            config.clone(),
            h.sender.clone() as Arc<dyn TelegramSender + Send + Sync>,
        )
    });

    // Relay mode: an idle tracker the RPC surface touches, plus the channel its monitor fires.
    let (idle_tracker, relay_idle_monitor, relay_shutdown) = match options.relay_idle_timeout {
        Some(timeout) => {
            let tracker = Arc::new(crate::relay_idle::IdleTimeoutTracker::new(timeout));
            let (tx, rx) = tokio::sync::oneshot::channel::<()>();
            (Some(Arc::clone(&tracker)), Some((tracker, tx)), Some(rx))
        }
        None => (None, None, None),
    };

    // The peer-discovery handles the roster is built from, when this daemon assembles discovery at
    // all. The common-room supervisor reconnects *these*, so the roster keeps pointing at the
    // registry and room slot it was built with.
    let mut peer_discovery: Option<PeerDiscoveryHandles> = None;

    let mut tasks = RuntimeTasks {
        common_room: None,
        oauth_loopback_tunnel: None,
        local_socket: None,
        lsp_idle_reaper: None,
        relay_idle_monitor,
        telegram_inbound: telegram.inbound,
    };

    if let Some(user_resolver) = auth_result.user_resolver {
        let config_arc = Arc::new(config.clone());
        // Peer discovery over the common room. The registry and the room slot are the handles the
        // roster is built from; the task that fills them is the host's to start.
        let livekit_discovery: Option<crate::livekit_peer_discovery::LiveKitDiscoveryHandles> =
            match CommonRoomTarget::from_livekit(config.livekit.as_ref()) {
                Some(target) => {
                    let registry =
                        Arc::new(crate::livekit_peer_discovery::CommonRoomPeerRegistry::new());
                    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
                    log::info!(
                        "LiveKit common-room peer discovery configured (room {:?})",
                        target.room()
                    );
                    peer_discovery = Some(PeerDiscoveryHandles {
                        registry: registry.clone(),
                        room_slot: room_slot.clone(),
                    });
                    tasks.oauth_loopback_tunnel = Some(OauthLoopbackTunnel {
                        config: config_arc.clone(),
                        room_slot: room_slot.clone(),
                    });
                    Some(crate::livekit_peer_discovery::LiveKitDiscoveryHandles {
                        eligible_daemon_source: Arc::new(
                            crate::livekit_peer_discovery::LiveKitEligibleDaemonSource::new(
                                config_arc.clone(),
                                registry,
                                room_slot.clone(),
                            ),
                        )
                            as Arc<dyn crate::multi_host::EligibleDaemonSource>,
                        common_room_livekit_room: room_slot,
                    })
                }
                None => None,
            };
        // Clone before moving into ConnectionServiceImpl — VmService and ScreenSharingService need the same resolver.
        let vm_user_resolver = user_resolver.clone();
        let sessions_base_resolver: crate::connection_service::SessionsBaseResolver = {
            let dd = tddy_data_dir.clone();
            Arc::new(move |user: &str| {
                crate::user_sessions_path::sessions_base_for_user(user, Some(&dd))
            })
        };
        let ss_user_resolver = user_resolver.clone();
        let remote_git_user_resolver = user_resolver.clone();
        // Every project a daemon serves is resolved against *that OS user's own* registry, so
        // this mirrors `sessions_base_resolver` one directory down.
        let projects_dir_resolver: crate::remote_git_service::ProjectsDirResolver = {
            let dd = tddy_data_dir.clone();
            Arc::new(move |user: &str| {
                crate::user_sessions_path::projects_path_for_user(user, Some(&dd))
            })
        };
        // Session-addressed BSP resolver: reproduce the ExecuteTool preamble (token → os_user →
        // sessions_base → `.session.yaml` repo_path) to yield a session's worktree + catalog dir.
        // Built here, before the resolvers are moved into ConnectionServiceImpl below.
        let bsp_session_resolver: crate::bsp_service::SessionPathsResolver = {
            let user_resolver = user_resolver.clone();
            let config = config.clone();
            let sessions_base_resolver = sessions_base_resolver.clone();
            Arc::new(move |token: &str, session_id: &str| {
                let github_user = (user_resolver)(token).ok_or_else(|| {
                    tddy_rpc::Status::unauthenticated("invalid or expired session")
                })?;
                let os_user = config
                    .os_user_for_github(&github_user)
                    .ok_or_else(|| {
                        tddy_rpc::Status::permission_denied("user not mapped to OS user")
                    })?
                    .to_string();
                tddy_core::session_lifecycle::validate_session_id_segment(session_id)
                    .map_err(|e| tddy_rpc::Status::invalid_argument(e.message()))?;
                let sessions_base = (sessions_base_resolver)(&os_user)
                    .ok_or_else(|| tddy_rpc::Status::internal("could not resolve sessions path"))?;
                let repo_root = crate::workspace_session::resolve_worktree_root_for_session(
                    &sessions_base,
                    session_id,
                )?;
                let session_dir = tddy_core::session_lifecycle::unified_session_dir_path(
                    &sessions_base,
                    session_id,
                );
                Ok((session_dir, repo_root))
            })
        };
        // Where a model chat may run an assistant's tools, for the operator holding the token.
        // Same preamble as `bsp_session_resolver` above (token → OS user → their own sessions
        // and projects): `NewSessionRequest.cwd` is client-chosen, and an assistant may be
        // assigned `Shell`, so the answer must be "the directories this caller already owns"
        // rather than "anywhere the daemon process can reach".
        let chat_workspace_roots: crate::model_registry::ChatWorkspaceRoots = {
            let user_resolver = user_resolver.clone();
            let config = config.clone();
            let sessions_base_resolver = sessions_base_resolver.clone();
            let projects_dir_resolver = projects_dir_resolver.clone();
            Arc::new(move |token: &str| {
                use crate::model_registry::ModelRegistryError;
                let github_user = (user_resolver)(token).ok_or_else(|| {
                    ModelRegistryError::PermissionDenied(
                        "invalid or expired session token".to_string(),
                    )
                })?;
                let os_user = config
                    .os_user_for_github(&github_user)
                    .ok_or_else(|| {
                        ModelRegistryError::PermissionDenied(
                            "user not mapped to OS user".to_string(),
                        )
                    })?
                    .to_string();
                let sessions_base = (sessions_base_resolver)(&os_user).ok_or_else(|| {
                    ModelRegistryError::InvalidWorkspace(
                        "could not resolve this operator's sessions path".to_string(),
                    )
                })?;
                let projects_dir = (projects_dir_resolver)(&os_user).ok_or_else(|| {
                    ModelRegistryError::InvalidWorkspace(
                        "could not resolve this operator's projects path".to_string(),
                    )
                })?;
                // Every session worktree this operator has, plus every project checkout their
                // own registry names — including per-host checkouts of the same project.
                let mut roots = vec![sessions_base.join("sessions")];
                let projects =
                    crate::project_storage::read_projects(&projects_dir).map_err(|e| {
                        ModelRegistryError::InvalidWorkspace(format!(
                            "could not read this operator's projects: {e}"
                        ))
                    })?;
                for project in projects {
                    roots.push(std::path::PathBuf::from(&project.main_repo_path));
                    roots.extend(
                        project
                            .host_repo_paths
                            .values()
                            .map(std::path::PathBuf::from),
                    );
                }
                Ok(roots)
            })
        };
        // This daemon's model registry: the providers it talks to, their models, and the
        // assistants composed from them. Opened before ConnectionService so an assistant is
        // listed by `ListAgents` as a selectable `--agent`.
        let model_registry = Arc::new(
            crate::model_registry::ModelRegistryStore::open(
                &tddy_data_dir.join("models.db"),
                &crate::livekit_peer_discovery::local_instance_id_for_config(&config),
                // The same directory `ConnectionServiceImpl` resolves YAML defs from, so an
                // assistant cannot be created under a name one of them already answers to.
                &tddy_data_dir.join("agents"),
            )
            .await?
            .reserving_agent_ids(config.allowed_agents().iter().map(|a| a.id.clone())),
        );

        // The daemon's session-notification bus: Telegram takes the attention-worthy events
        // from the activity-status path, and `StreamSessionNotifications` relays every event
        // to the browsers driving the drawer's indicators. Assembled here rather than left to
        // `ConnectionServiceImpl::new` (which would build a Telegram-only bus) because the
        // stream subscriber must be the very one the RPC handler subscribes to.
        let session_notification_bus = {
            let mut bus = crate::session_notifications::SessionNotificationBus::new();
            if let Some(ref hooks) = telegram_hooks {
                bus = bus.with_subscriber(Arc::new(
                    crate::session_notification_subscribers::TelegramNotificationSubscriber::new(
                        Arc::clone(hooks),
                    ),
                ));
            }
            Arc::new(bus.with_subscriber(Arc::new(
                crate::session_notification_subscribers::SessionNotificationStreamSubscriber::new(),
            )))
        };

        let mut connection_impl = crate::connection_service::ConnectionServiceImpl::new(
            config.clone(),
            sessions_base_resolver,
            tddy_data_dir.clone(),
            user_resolver,
            options.spawn_client.clone(),
            livekit_discovery,
            telegram_hooks.clone(),
            Arc::clone(&shared_claude_cli_manager),
        )
        .with_session_rooms(Arc::clone(&shared_session_rooms))
        .with_model_registry(Arc::clone(&model_registry))
        .with_session_notification_bus(session_notification_bus);
        if let Some(ref tracker) = idle_tracker {
            connection_impl = connection_impl.with_idle_tracker(tracker.clone());
        }
        if let Some(store) = auth_result.github_token_store.clone() {
            connection_impl = connection_impl.with_github_token_store(store);
        }
        // Share one instance across transports: the LiveKit/HTTP RpcService server and the
        // local Unix-domain-socket tonic server both reference the same Arc, so a session
        // started over the socket is visible over every other transport.
        let connection_arc = Arc::new(connection_impl);
        // Record the self-handle so `&self` methods (the sandbox-IPC RPC bridge, reached via
        // `dial_and_bridge`) can recover this `Arc` and hand it to the in-jail `tddy-tools`'s
        // roster/conversation dispatch. Set on the original; shared across `Clone`s.
        connection_arc.set_self_handle(Arc::downgrade(&connection_arc));
        // Get the shared TaskRegistry before handing the impl to the servers.
        let task_registry = connection_arc.task_registry();
        // The admission registry is shared with the SessionAdmissionService served on the
        // common room (PRD § "What attach does" step 3); capture it before `connection_arc`
        // moves into the ConnectionServiceServer below.
        let session_admissions = connection_arc.session_admissions();

        // Register the build-catalog provider so a populated session catalog includes the
        // repository's `BUILD.yaml` targets (discovery lives in `tddy-bsp` on top of
        // `tddy-build`; `tddy-core` owns only the port).
        tddy_bsp::register_catalog_provider();

        // Reusable-LSP executor: a Rust-only executor sharing this daemon's task registry,
        // so `Lsp*` tool calls (relayed through tddy-tool-engine) resolve to a real, reused
        // language server; the loop that reaps servers left idle is the host's to start.
        tasks.lsp_idle_reaper = Some(tddy_lsp_executor::register(
            task_registry.clone(),
            tddy_lsp::LspAllowList::rust_only(),
            Duration::from_secs(300),
        ));

        // Local Unix-domain socket transport (SO_PEERCRED peer-trust + MintLocalToken). Served by
        // the binary host only: the socket path names one daemon, and a systemd-activated listener
        // is addressed to the binary's pid.
        if options.host == RuntimeHost::Binary {
            // The one secret every daemon shares (also signs session tokens); when absent the
            // adapter denies minting with FAILED_PRECONDITION.
            let signer = config_arc
                .livekit
                .as_ref()
                .and_then(|lk| lk.api_secret.clone())
                .map(|s| tddy_github::SessionTokenSigner::new(s.as_bytes()));
            let uid_to_username: crate::connection_tonic_adapter::UidToUsername =
                Arc::new(crate::user_sessions_path::username_for_uid);
            tasks.local_socket = Some(LocalSocketTransport {
                socket_path: config_arc.local_socket_path(),
                adapter: crate::connection_tonic_adapter::ConnectionServiceTonicAdapter::new(
                    connection_arc.clone(),
                    config_arc.clone(),
                    signer,
                    uid_to_username,
                ),
            });
        }

        let connection_server = tddy_service::ConnectionServiceServer::from_arc(connection_arc);
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "connection.ConnectionService",
            service: Arc::new(connection_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // ModelRegistryService — this daemon's providers, models and assistants. Rides the same
        // entries as every other service, so it is reachable over HTTP `/rpc` and LiveKit alike.
        let model_registry_server = tddy_service::ModelRegistryServiceServer::new(
            crate::model_registry::ModelRegistryServiceImpl::new(
                Arc::clone(&model_registry),
                Arc::new(crate::model_registry::DefaultProviderClients),
                vm_user_resolver.clone(),
            ),
        );
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "models.ModelRegistryService",
            service: Arc::new(model_registry_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // The model-addressed ACP surface: chatting with a registry model or assistant. The
        // *session*-addressed `acp.AcpService` is mounted per session process
        // (`session_view_adapter_surface`); this one is the daemon's own, so the Models & Agents
        // screen can open a chat without a session existing at all.
        let model_acp_server =
            tddy_service::AcpServiceServer::new(crate::model_registry::ModelAcpService::new(
                Arc::clone(&model_registry),
                task_registry.clone(),
                vm_user_resolver.clone(),
                chat_workspace_roots,
            ));
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: tddy_service::AcpServiceServer::<crate::model_registry::ModelAcpService>::NAME,
            service: Arc::new(model_acp_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // TaskService — backed by the same registry as ConnectionService.
        let task_service_impl = crate::task_service::TaskServiceImpl::new(
            task_registry.clone(),
            vm_user_resolver.clone(),
        );
        let task_server = tddy_service::TaskServiceServer::new(task_service_impl);
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "tasks.TaskService",
            service: Arc::new(task_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // RemoteGitService — every project this daemon serves, usable as a git remote by any
        // client that can join the room (`GIT_SSH_COMMAND=tddy-remote-git-repo`).
        let remote_git_server = tddy_service::RemoteGitServiceServer::new(
            crate::remote_git_service::RemoteGitServiceImpl::new(
                remote_git_user_resolver.clone(),
                projects_dir_resolver,
                config_arc.clone(),
            ),
        );
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "remote_git.RemoteGitService",
            service: Arc::new(remote_git_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // SessionAdmissionService — the room-admission handshake (PRD § "What attach does"
        // step 3). The facilitating daemon mints a scoped short-TTL token for an owning daemon
        // it has admitted, and revokes it when the last agent that daemon owns detaches. Served
        // on the common-room `daemon-{this}` participant so an owning daemon that is in the
        // session room (but whose token is expiring) can still reach this daemon over the
        // common room it never left.
        let session_admission_server = tddy_service::SessionAdmissionServiceServer::new(
            crate::session_admission_service::SessionAdmissionServiceImpl::new(
                remote_git_user_resolver.clone(),
                config_arc.clone(),
                session_admissions,
                Arc::new({
                    let rooms = Arc::clone(&shared_session_rooms);
                    move |session_id: &str| rooms.contains(session_id)
                }),
            ),
        );
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "session_admission.SessionAdmissionService",
            service: Arc::new(session_admission_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // ActionService — start tools by kind via tddy-actions runtimes.
        let action_service_impl = crate::action_service::ActionServiceImpl::new(
            task_registry.clone(),
            tddy_actions::ActionCatalog::new(),
            vm_user_resolver.clone(),
        );
        let action_server = tddy_service::ActionServiceServer::new(action_service_impl);
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "actions.ActionService",
            service: Arc::new(action_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // BSP build server — session-addressed: each request's token/session_id resolves to that
        // session's worktree + catalog.db (`bsp_service`), so daemon-managed claude-cli/cursor
        // sessions expose build targets over the same surface as ConnectionService.
        let bsp_server = tddy_service::BspServiceServer::new(
            crate::bsp_service::DaemonBspService::new(bsp_session_resolver, tddy_data_dir.clone()),
        );
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "bsp.BspService",
            service: Arc::new(bsp_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // VM lifecycle service — gated on auth being configured (same as ConnectionService).
        // Per-VM manifest files under the VM & Image Library are the source of truth
        // (superseding the old single shared vm-registry.json); rooted at the same
        // config-only tddy data dir every other per-user store here resolves from.
        let vm_library = {
            let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
            let base = crate::user_sessions_path::tddy_data_root_matching_child(
                &user,
                Some(&tddy_data_dir),
            )
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
            let library = tddy_vm::VmLibrary::new(base);
            if let Err(e) = library.init() {
                log::error!(
                    "Failed to initialize VM & Image Library at {}: {e}",
                    library.root().display()
                );
            }
            library
        };
        let vm_manager = Arc::new(tddy_vm::VmManager::from_library(
            vm_library,
            Box::new(tddy_vm::QemuVm),
        ));
        let vm_service_impl =
            tddy_vm::VmServiceImpl::new(Arc::clone(&vm_manager), vm_user_resolver, task_registry);
        let vm_server = tddy_service::VmServiceServer::new(vm_service_impl);
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "vm.VmService",
            service: Arc::new(vm_server) as Arc<dyn tddy_rpc::RpcService>,
        });

        // Screen sharing service — vault management + VNC/RDP bridge spawning.
        // Wire `sessions_base` with the daemon's resolved `tddy_data_dir` so vaults live
        // under the config-only tddy home (config → profile default → `$HOME/.tddy`),
        // matching `sessions_base_resolver` above — not a statically-derived `$HOME/.tddy`.
        let ss_key_cache: crate::screen_sharing_service::ScreenSharingKeyCache =
            Arc::new(Mutex::new(HashMap::new()));
        let ss_sessions_base: crate::screen_sharing_service::SessionsBase = {
            let dd = tddy_data_dir.clone();
            Arc::new(move |user: &str| {
                crate::user_sessions_path::sessions_base_for_user(user, Some(&dd))
            })
        };
        let ss_svc = crate::screen_sharing_service::ScreenSharingServiceImpl::new(
            ss_user_resolver,
            ss_sessions_base,
            Arc::clone(&ss_key_cache),
        )
        .with_config(Arc::clone(&config_arc));
        let ss_server = tddy_service::ScreenSharingServiceServer::new(ss_svc);
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "screen_sharing.ScreenSharingService",
            service: Arc::new(ss_server) as Arc<dyn tddy_rpc::RpcService>,
        });
    }

    // The daemon's own settings, read and written by its UI. Registered for every host — a desktop
    // application embedding the daemon is the one that most needs to configure it — and gated on the
    // same session token as every other daemon RPC.
    let config_service = DaemonConfigServiceServer::new(DaemonConfigServiceImpl::new(
        options.config_path.clone(),
        Arc::new(Mutex::new(config.clone())),
        Arc::clone(&common_room) as Arc<dyn CommonRoomSupervisor>,
        config_service_authenticator,
    ));
    rpc_entries.push(tddy_rpc::ServiceEntry {
        name: DaemonConfigServiceServer::<DaemonConfigServiceImpl>::NAME,
        service: Arc::new(config_service) as Arc<dyn tddy_rpc::RpcService>,
    });

    let service_name_strs: Vec<&str> = rpc_entries.iter().map(|e| e.name).collect();
    rpc_entries.push(tddy_service::reflection_entry_from(&service_name_strs));

    // Serve the daemon's RPC services on the LiveKit common room, so a client that can join the
    // room can invoke every service without an HTTP origin — and keep serving them on whatever
    // room the configuration names next. Joining is the host's to start: assembling a runtime
    // dials nothing.
    tasks.common_room = Some(common_room.task(Arc::new(DaemonCommonRoomConnector::new(
        config.clone(),
        cloned_entries(&rpc_entries),
        peer_discovery,
    ))));

    Ok(DaemonRuntime {
        entries: rpc_entries,
        config,
        cli_sessions: shared_claude_cli_manager,
        lifecycle_telegram,
        relay_shutdown,
        tasks,
    })
}

/// What a configured Telegram bot contributes: the hooks every session-notification path publishes
/// through, and the inbound dispatcher that serves the buttons an operator presses.
struct TelegramWiring {
    hooks: Option<Arc<crate::telegram_session_subscriber::TelegramDaemonHooks>>,
    inbound: Option<TelegramInbound>,
}

fn build_telegram(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    options: &RuntimeOptions,
    claude_cli_manager: &Arc<crate::cli_session_manager::CliSessionManager>,
    session_rooms: &Arc<crate::session_room::SessionRoomRegistry>,
) -> TelegramWiring {
    let tg = match config.telegram.as_ref() {
        Some(tg) if tg.enabled && !tg.bot_token.is_empty() => tg,
        _ => {
            return TelegramWiring {
                hooks: None,
                inbound: None,
            }
        }
    };

    let bot = Bot::new(tg.bot_token.clone());
    let teloxide_sender = Arc::new(crate::telegram_notifier::TeloxideSender::new(bot.clone()));
    let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
    let sender: Arc<dyn TelegramSender + Send + Sync> = teloxide_sender.clone();
    let elicitation_select_options: crate::telegram_notifier::ElicitationSelectOptionsCache =
        Arc::new(StdMutex::new(HashMap::new()));
    let elicitation_multi_select_meta: crate::telegram_notifier::ElicitationMultiSelectMetaCache =
        Arc::new(StdMutex::new(HashMap::new()));
    let active_elicitation = Arc::new(StdMutex::new(
        crate::active_elicitation::ActiveElicitationCoordinator::new(),
    ));
    let telegram_tracked = Arc::new(StdMutex::new(
        crate::telegram_tracked_session::TelegramTrackedSessionCoordinator::new(),
    ));
    let watcher = Arc::new(Mutex::new(
        crate::telegram_notifier::TelegramSessionWatcher::with_elicitation_caches_coordinator_and_tracked(
            elicitation_select_options.clone(),
            elicitation_multi_select_meta.clone(),
            active_elicitation.clone(),
            telegram_tracked.clone(),
        ),
    ));
    let hooks = Arc::new(crate::telegram_session_subscriber::TelegramDaemonHooks {
        config: config.clone(),
        sender: sender.clone(),
        watcher,
    });

    let sessions_base = match crate::user_sessions_path::tddy_data_root_matching_child(
        &user,
        Some(tddy_data_dir),
    ) {
        Some(base) => base,
        None => {
            log::warn!(
                target: "tddy_daemon",
                "telegram inbound session control disabled: could not resolve sessions base for USER={user}"
            );
            return TelegramWiring {
                hooks: Some(hooks),
                inbound: None,
            };
        }
    };

    #[cfg(unix)]
    let spawn_for_tg = options
        .spawn_client
        .as_ref()
        .map(|(c, _)| Arc::new(c.clone()));
    #[cfg(not(unix))]
    let spawn_for_tg: Option<Arc<crate::spawn_worker::SpawnClient>> = {
        let _ = options;
        None
    };

    let workflow_spawn = Some(Arc::new(
        crate::telegram_session_control::TelegramWorkflowSpawn {
            config: Arc::new(config.clone()),
            spawn_client: spawn_for_tg,
            os_user: user.clone(),
            tddy_data_dir: tddy_data_dir.to_path_buf(),
            projects_dir_override: None,
            telegram_hooks: Some(hooks.clone()),
            child_grpc_by_session: Arc::new(StdMutex::new(HashMap::new())),
            elicitation_select_options: elicitation_select_options.clone(),
            elicitation_multi_select_meta: elicitation_multi_select_meta.clone(),
            pending_elicitation_other: Arc::new(StdMutex::new(HashMap::new())),
            claude_cli_manager: Arc::clone(claude_cli_manager),
        },
    ));
    let harness = Arc::new(Mutex::new(
        crate::telegram_session_control::TelegramSessionControlHarness::with_workflow_spawn_and_telegram_tracked(
            tg.chat_ids.clone(),
            sessions_base,
            teloxide_sender,
            workflow_spawn,
            Some(active_elicitation),
            Some(telegram_tracked),
        )
        .with_session_rooms(Arc::clone(session_rooms)),
    ));

    TelegramWiring {
        hooks: Some(hooks),
        inbound: Some(TelegramInbound { bot, harness }),
    }
}

#[cfg(test)]
mod data_dir {
    use super::*;

    /// A daemon whose configuration names where its state lives.
    fn a_daemon_keeping_state_in(path: &str) -> DaemonConfig {
        DaemonConfig {
            tddy_data_dir: Some(PathBuf::from(path)),
            ..DaemonConfig::default()
        }
    }

    #[test]
    fn keeps_state_where_the_configuration_says() {
        // Given a configuration naming a data directory, and no override
        let config = a_daemon_keeping_state_in("tmp/.tddy");

        // When the data directory is resolved
        let resolved = data_dir_from(&config, None);

        // Then the configured directory is used
        assert_eq!(resolved, Some(PathBuf::from("tmp/.tddy")));
    }

    #[test]
    fn lets_the_environment_move_state_off_the_configured_directory() {
        // Given a configuration naming one directory and an environment naming another — which is
        // how a development run is kept out of the `~/.tddy` an installed app owns
        let config = a_daemon_keeping_state_in("/var/lib/tddy");

        // When the data directory is resolved
        let resolved = data_dir_from(&config, Some("tmp/.tddy".to_string()));

        // Then the environment wins
        assert_eq!(resolved, Some(PathBuf::from("tmp/.tddy")));
    }

    #[test]
    fn ignores_an_empty_override_rather_than_keeping_state_in_no_directory() {
        // Given an override that is set but empty, as an unset shell variable expands to
        let config = a_daemon_keeping_state_in("tmp/.tddy");

        // When the data directory is resolved
        let resolved = data_dir_from(&config, Some("   ".to_string()));

        // Then it is ignored: an empty path would put a daemon's state at the filesystem root
        assert_eq!(resolved, Some(PathBuf::from("tmp/.tddy")));
    }

    #[test]
    fn falls_back_to_the_builds_default_when_nothing_names_a_directory() {
        // Given no configured directory and no override
        let config = DaemonConfig::default();

        // When the data directory is resolved
        let resolved = data_dir_from(&config, None);

        // Then nothing is chosen here — the caller applies the build's default
        assert_eq!(resolved, None);
    }
}
