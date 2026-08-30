//! tddy-daemon — multi-user daemon for tddy-* tools.
//!
//! Runs as root process. Handles GitHub auth, user mapping, session discovery,
//! and spawns tddy-* processes as the target OS user.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use clap::Parser;
use teloxide::prelude::Bot;
use tokio::sync::Mutex;

/// Apply environment variable overrides to config (e.g. from .env loaded by web-dev).
///
/// Also sets `codex_oauth_loopback_proxy_eligible` from `TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE` when present.
fn apply_env_overrides(config: &mut tddy_daemon::config::DaemonConfig) {
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

#[derive(Parser, Debug)]
#[command(name = "tddy-daemon")]
#[command(about = "Multi-user daemon for tddy-* tools")]
struct Args {
    /// Path to config file (YAML)
    #[arg(short, long, env = "TDDY_DAEMON_CONFIG")]
    config: Option<PathBuf>,

    /// Run in relay mode: no web bundle required, idle-timeout auto-shutdown,
    /// forwards RPCs to a remote peer via LiveKit.
    #[arg(long)]
    relay: bool,
}

fn main() -> anyhow::Result<()> {
    // Ignore SIGPIPE — writing to spawn worker pipe after it dies would otherwise crash the daemon
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    let args = Args::parse();
    let config_path = args
        .config
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--config is required"))?;

    let mut config = tddy_daemon::config::DaemonConfig::load(config_path)?;

    let log_config = config
        .log
        .clone()
        .unwrap_or_else(|| tddy_core::default_log_config(None, None));
    tddy_core::init_tddy_logger(log_config);

    log::info!("tddy-daemon loaded config from {}", config_path.display());

    // Fork spawn worker before tokio — fork() from multi-threaded process can deadlock. Skipped
    // entirely on a supervised host: there, `tddy-supervisor` spawns sessions, and a worker the
    // daemon could reach for would be a way to spawn one with less isolation than it asked for.
    let spawn_backend = tddy_daemon::supervisor_client::spawn_backend_choice(&config);
    let spawn_client = tddy_daemon::supervisor_client::spawn_worker_for(&spawn_backend)?;
    #[cfg(unix)]
    if let Some((_, worker_pid)) = spawn_client.as_ref() {
        log::info!(
            "spawn worker pid={} (strace while debugging spawns: sudo strace -f -tt -T -p {})",
            worker_pid,
            worker_pid
        );
    }

    // Apply env overrides (e.g. from .env loaded by web-dev)
    apply_env_overrides(&mut config);

    // Scope git's ssh command to this daemon — applied to remote fetches only, without polluting the
    // process environment or global git config. See DaemonConfig::git / GitConfig::ssh_command.
    tddy_core::set_git_ssh_command(config.git.as_ref().and_then(|g| g.ssh_command.clone()));

    // Resolve the tddy home data directory: config is the single source of truth.
    let tddy_data_dir: PathBuf = config
        .tddy_data_dir
        .clone()
        .or_else(tddy_core::output::default_tddy_data_dir)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".tddy")
        });

    let (port, bundle_path_opt) = tddy_daemon::startup::startup_config_check(&config, args.relay)?;
    let host = config
        .listen
        .web_host
        .clone()
        .unwrap_or_else(|| "0.0.0.0".to_string());
    log::info!("tddy-daemon listening on {}:{}", host, port);
    // In relay mode bundle_path is None; in non-relay mode startup_config_check already
    // guaranteed it is Some (returning Err otherwise). Unwrap is safe for non-relay path.
    let bundle_path = bundle_path_opt.unwrap_or_else(|| PathBuf::from(""));

    let livekit_url = config
        .livekit
        .as_ref()
        .and_then(|l| l.public_url.clone())
        .or_else(|| config.livekit.as_ref().and_then(|l| l.url.clone()));

    let common_room = config.livekit.as_ref().and_then(|l| l.common_room.clone());

    // Browser DEBUG mask (debug-package namespaces) exposed at /api/config; see DaemonConfig::debug.
    let web_debug = config.debug.clone();

    // The startup snapshot the web bundle is served with. Config entries only: assistants are
    // created and deleted while the daemon runs, so no snapshot could stay right about them —
    // `ListAgents` is their live source, and it reads the registry on every call.
    let allowed_agents: Vec<tddy_coder::web_server::ClientAllowedAgent> =
        tddy_daemon::agent_list_mapping::agent_allowlist_rows(&config, &[])
            .into_iter()
            .map(|row| tddy_coder::web_server::ClientAllowedAgent {
                id: row.id,
                label: row.display_label,
            })
            .collect();

    let auth_result = tddy_daemon::auth::build_auth_entries(&config, host.as_str(), port)?;
    let mut rpc_entries = auth_result.entries;

    // The room-JWT mint the web UI joins rooms through. Gated on the same session token as every
    // other daemon RPC — anything that can reach `/rpc` can call it, and a JWT it minted is
    // admission to a LiveKit room. See `tddy_daemon::auth::build_token_service_entry`.
    if let Some(entry) =
        tddy_daemon::auth::build_token_service_entry(&config, auth_result.user_resolver.as_ref())
    {
        rpc_entries.push(entry);
    }

    // Create one shared ClaudeCliSessionManager — injected into both the Telegram spawn path and
    // ConnectionServiceImpl so that Telegram-launched sessions are attachable via the terminal RPCs.
    let shared_claude_cli_manager =
        Arc::new(tddy_daemon::cli_session_manager::CliSessionManager::new());

    // One registry of hosted session rooms for the whole daemon: `StartSession` opens rooms in it
    // and both deletion paths — the `DeleteSession` RPC and Telegram's Delete button — close them
    // there. Two registries would mean a room only one of them could ever stop hosting.
    let shared_session_rooms = Arc::new(tddy_daemon::session_room::SessionRoomRegistry::new());

    let mut telegram_inbound: Option<(
        Bot,
        Arc<
            Mutex<
                tddy_daemon::telegram_session_control::TelegramSessionControlHarness<
                    tddy_daemon::telegram_notifier::TeloxideSender,
                >,
            >,
        >,
    )> = None;

    let telegram_hooks: Option<Arc<tddy_daemon::telegram_session_subscriber::TelegramDaemonHooks>> =
        match config.telegram.as_ref() {
            Some(tg) if tg.enabled && !tg.bot_token.is_empty() => {
                let bot = Bot::new(tg.bot_token.clone());
                let teloxide_sender = Arc::new(
                    tddy_daemon::telegram_notifier::TeloxideSender::new(bot.clone()),
                );
                let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
                let sender: Arc<dyn tddy_daemon::telegram_notifier::TelegramSender + Send + Sync> =
                    teloxide_sender.clone();
                let elicitation_select_options: tddy_daemon::telegram_notifier::ElicitationSelectOptionsCache =
                    Arc::new(StdMutex::new(HashMap::new()));
                let elicitation_multi_select_meta: tddy_daemon::telegram_notifier::ElicitationMultiSelectMetaCache =
                    Arc::new(StdMutex::new(HashMap::new()));
                let active_elicitation = Arc::new(StdMutex::new(
                    tddy_daemon::active_elicitation::ActiveElicitationCoordinator::new(),
                ));
                let telegram_tracked = Arc::new(StdMutex::new(
                    tddy_daemon::telegram_tracked_session::TelegramTrackedSessionCoordinator::new(),
                ));
                let watcher = Arc::new(Mutex::new(
                    tddy_daemon::telegram_notifier::TelegramSessionWatcher::with_elicitation_caches_coordinator_and_tracked(
                        elicitation_select_options.clone(),
                        elicitation_multi_select_meta.clone(),
                        active_elicitation.clone(),
                        telegram_tracked.clone(),
                    ),
                ));
                let hooks = Arc::new(
                    tddy_daemon::telegram_session_subscriber::TelegramDaemonHooks {
                        config: config.clone(),
                        sender: sender.clone(),
                        watcher,
                    },
                );
                if let Some(sessions_base) =
                    tddy_daemon::user_sessions_path::tddy_data_root_matching_child(
                        &user,
                        Some(&tddy_data_dir),
                    )
                {
                    #[cfg(unix)]
                    let spawn_for_tg = spawn_client.as_ref().map(|(c, _)| Arc::new(c.clone()));
                    #[cfg(not(unix))]
                    let spawn_for_tg: Option<
                        Arc<tddy_daemon::spawn_worker::SpawnClient>,
                    > = None;

                    let workflow_spawn = Some(Arc::new(
                        tddy_daemon::telegram_session_control::TelegramWorkflowSpawn {
                            config: Arc::new(config.clone()),
                            spawn_client: spawn_for_tg,
                            os_user: user.clone(),
                            tddy_data_dir: tddy_data_dir.clone(),
                            projects_dir_override: None,
                            telegram_hooks: Some(hooks.clone()),
                            child_grpc_by_session: Arc::new(StdMutex::new(HashMap::new())),
                            elicitation_select_options: elicitation_select_options.clone(),
                            elicitation_multi_select_meta: elicitation_multi_select_meta.clone(),
                            pending_elicitation_other: Arc::new(StdMutex::new(HashMap::new())),
                            claude_cli_manager: Arc::clone(&shared_claude_cli_manager),
                        },
                    ));
                    let harness = Arc::new(Mutex::new(
                        tddy_daemon::telegram_session_control::TelegramSessionControlHarness::with_workflow_spawn_and_telegram_tracked(
                            tg.chat_ids.clone(),
                            sessions_base,
                            teloxide_sender,
                            workflow_spawn,
                            Some(active_elicitation),
                            Some(telegram_tracked),
                        )
                        .with_session_rooms(Arc::clone(&shared_session_rooms)),
                    ));
                    telegram_inbound = Some((bot.clone(), harness));
                } else {
                    log::warn!(
                        target: "tddy_daemon",
                        "telegram inbound session control disabled: could not resolve sessions base for USER={user}"
                    );
                }
                Some(hooks)
            }
            _ => None,
        };

    let user_resolver_for_connection = auth_result.user_resolver.clone();
    // The GitHub token each web login granted, shared with ConnectionService so PR-status reads act
    // with the calling operator's own credential.
    let github_token_store_for_connection = auth_result.github_token_store.clone();

    // In relay mode, wire up the idle-timeout tracker + monitor task + external shutdown channel.
    let relay_idle_timeout: Option<std::time::Duration> = if args.relay {
        config
            .relay
            .as_ref()
            .map(|r| std::time::Duration::from_secs(r.idle_timeout_secs))
    } else {
        None
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let lifecycle_telegram = telegram_hooks.as_ref().map(|h| {
        (
            config.clone(),
            h.sender.clone()
                as Arc<dyn tddy_daemon::telegram_notifier::TelegramSender + Send + Sync>,
        )
    });
    rt.block_on(async move {
        // Relay mode: create idle tracker + external shutdown channel.
        // Must be in the outer scope so idle_rx_opt and idle_tx_opt are accessible after the
        // `if let Some(user_resolver)` block (which pushes rpc_entries before run_server).
        let (idle_tracker_opt, idle_rx_opt, idle_tx_opt) = if let Some(timeout) = relay_idle_timeout
        {
            let tracker = Arc::new(tddy_daemon::relay_idle::IdleTimeoutTracker::new(timeout));
            let (tx, rx) = tokio::sync::oneshot::channel::<()>();
            (Some(tracker), Some(rx), Some(tx))
        } else {
            (
                None::<Arc<tddy_daemon::relay_idle::IdleTimeoutTracker>>,
                None::<tokio::sync::oneshot::Receiver<()>>,
                None::<tokio::sync::oneshot::Sender<()>>,
            )
        };

        if let Some(user_resolver) = user_resolver_for_connection {
            let config_arc = Arc::new(config.clone());
            let livekit_discovery: Option<
                tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles,
            > = {
                let common = config
                    .livekit
                    .as_ref()
                    .and_then(|l| l.common_room.as_deref())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let lk = config.livekit.as_ref();
                let has_creds = lk.is_some_and(|l| {
                    l.url
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .is_some()
                        && l.api_key
                            .as_deref()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .is_some()
                        && l.api_secret
                            .as_deref()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .is_some()
                });
                if common.is_some() && has_creds {
                    let registry = Arc::new(
                        tddy_daemon::livekit_peer_discovery::CommonRoomPeerRegistry::new(),
                    );
                    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
                    log::info!(
                        "Starting LiveKit common-room peer discovery (room {:?})",
                        common
                    );
                    tddy_daemon::livekit_peer_discovery::spawn_common_room_discovery_task(
                        config_arc.clone(),
                        registry.clone(),
                        room_slot.clone(),
                    );
                    Some(tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles {
                        eligible_daemon_source: Arc::new(
                            tddy_daemon::livekit_peer_discovery::LiveKitEligibleDaemonSource::new(
                                config_arc.clone(), registry, room_slot.clone(),
                            ),
                        )
                            as Arc<dyn tddy_daemon::multi_host::EligibleDaemonSource>,
                        common_room_livekit_room: room_slot,
                    })
                } else {
                    None
                }
            };
            // Clone before moving into ConnectionServiceImpl — VmService and ScreenSharingService need the same resolver.
            let vm_user_resolver = user_resolver.clone();
            let sessions_base_resolver: tddy_daemon::connection_service::SessionsBaseResolver = {
                let dd = tddy_data_dir.clone();
                Arc::new(move |user: &str| {
                    tddy_daemon::user_sessions_path::sessions_base_for_user(user, Some(&dd))
                })
            };
            let ss_user_resolver = user_resolver.clone();
            let remote_git_user_resolver = user_resolver.clone();
            // Every project a daemon serves is resolved against *that OS user's own* registry, so
            // this mirrors `sessions_base_resolver` one directory down.
            let projects_dir_resolver: tddy_daemon::remote_git_service::ProjectsDirResolver = {
                let dd = tddy_data_dir.clone();
                Arc::new(move |user: &str| {
                    tddy_daemon::user_sessions_path::projects_path_for_user(user, Some(&dd))
                })
            };
            // Session-addressed BSP resolver: reproduce the ExecuteTool preamble (token → os_user →
            // sessions_base → `.session.yaml` repo_path) to yield a session's worktree + catalog dir.
            // Built here, before the resolvers are moved into ConnectionServiceImpl below.
            let bsp_session_resolver: tddy_daemon::bsp_service::SessionPathsResolver = {
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
                    let sessions_base = (sessions_base_resolver)(&os_user).ok_or_else(|| {
                        tddy_rpc::Status::internal("could not resolve sessions path")
                    })?;
                    let repo_root =
                        tddy_daemon::workspace_session::resolve_worktree_root_for_session(
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
            let chat_workspace_roots: tddy_daemon::model_registry::ChatWorkspaceRoots = {
                let user_resolver = user_resolver.clone();
                let config = config.clone();
                let sessions_base_resolver = sessions_base_resolver.clone();
                let projects_dir_resolver = projects_dir_resolver.clone();
                Arc::new(move |token: &str| {
                    use tddy_daemon::model_registry::ModelRegistryError;
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
                    let projects = tddy_daemon::project_storage::read_projects(&projects_dir)
                        .map_err(|e| {
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
                tddy_daemon::model_registry::ModelRegistryStore::open(
                    &tddy_data_dir.join("models.db"),
                    &tddy_daemon::livekit_peer_discovery::local_instance_id_for_config(&config),
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
                let mut bus = tddy_daemon::session_notifications::SessionNotificationBus::new();
                if let Some(ref hooks) = telegram_hooks {
                    bus = bus.with_subscriber(Arc::new(
                        tddy_daemon::session_notification_subscribers::TelegramNotificationSubscriber::new(
                            Arc::clone(hooks),
                        ),
                    ));
                }
                Arc::new(bus.with_subscriber(Arc::new(
                    tddy_daemon::session_notification_subscribers::SessionNotificationStreamSubscriber::new(),
                )))
            };

            let mut connection_impl = tddy_daemon::connection_service::ConnectionServiceImpl::new(
                config.clone(),
                sessions_base_resolver,
                tddy_data_dir.clone(),
                user_resolver,
                spawn_client,
                livekit_discovery,
                telegram_hooks.clone(),
                Arc::clone(&shared_claude_cli_manager),
            )
            .with_session_rooms(Arc::clone(&shared_session_rooms))
            .with_model_registry(Arc::clone(&model_registry))
            .with_session_notification_bus(session_notification_bus);
            if let Some(ref tracker) = idle_tracker_opt {
                connection_impl = connection_impl.with_idle_tracker(tracker.clone());
            }
            if let Some(store) = github_token_store_for_connection.clone() {
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
            // language server; a background loop reaps servers left idle.
            {
                let lsp_registry = tddy_lsp_executor::register(
                    task_registry.clone(),
                    tddy_lsp::LspAllowList::rust_only(),
                    std::time::Duration::from_secs(300),
                );
                tokio::spawn(async move {
                    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
                    ticker.tick().await; // consume the immediate first tick
                    loop {
                        ticker.tick().await;
                        lsp_registry.reap_idle().await;
                    }
                });
            }

            // Local Unix-domain socket transport (SO_PEERCRED peer-trust + MintLocalToken). Spawned
            // as an independent task; it must not disturb the HTTP server below.
            {
                let socket_path = config_arc.local_socket_path();
                // The one secret every daemon shares (also signs session tokens); when absent the
                // adapter denies minting with FAILED_PRECONDITION.
                let signer = config_arc
                    .livekit
                    .as_ref()
                    .and_then(|lk| lk.api_secret.clone())
                    .map(|s| tddy_github::SessionTokenSigner::new(s.as_bytes()));
                let uid_to_username: tddy_daemon::connection_tonic_adapter::UidToUsername =
                    Arc::new(tddy_daemon::user_sessions_path::username_for_uid);
                let adapter =
                    tddy_daemon::connection_tonic_adapter::ConnectionServiceTonicAdapter::new(
                        connection_arc.clone(),
                        config_arc.clone(),
                        signer,
                        uid_to_username,
                    );
                tokio::spawn(async move {
                    let shutdown = async {
                        let mut term = tokio::signal::unix::signal(
                            tokio::signal::unix::SignalKind::terminate(),
                        )
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
                    if let Err(e) = tddy_daemon::local_socket_server::serve_connection_uds(
                        &socket_path,
                        adapter,
                        shutdown,
                    )
                    .await
                    {
                        log::error!(
                            target: "tddy_daemon::local_socket_server",
                            "local socket server exited with error: {e:#}"
                        );
                    }
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
                tddy_daemon::model_registry::ModelRegistryServiceImpl::new(
                    Arc::clone(&model_registry),
                    Arc::new(tddy_daemon::model_registry::DefaultProviderClients),
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
            let model_acp_server = tddy_service::AcpServiceServer::new(
                tddy_daemon::model_registry::ModelAcpService::new(
                    Arc::clone(&model_registry),
                    task_registry.clone(),
                    vm_user_resolver.clone(),
                    chat_workspace_roots,
                ),
            );
            rpc_entries.push(
                tddy_rpc::ServiceEntry {
                    name: tddy_service::AcpServiceServer::<
                        tddy_daemon::model_registry::ModelAcpService,
                    >::NAME,
                    service: Arc::new(model_acp_server) as Arc<dyn tddy_rpc::RpcService>,
                },
            );

            // TaskService — backed by the same registry as ConnectionService.
            let task_service_impl = tddy_daemon::task_service::TaskServiceImpl::new(
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
                tddy_daemon::remote_git_service::RemoteGitServiceImpl::new(
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
                tddy_daemon::session_admission_service::SessionAdmissionServiceImpl::new(
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
            let action_service_impl = tddy_daemon::action_service::ActionServiceImpl::new(
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
                tddy_daemon::bsp_service::DaemonBspService::new(
                    bsp_session_resolver,
                    tddy_data_dir.clone(),
                ),
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
                let base = tddy_daemon::user_sessions_path::tddy_data_root_matching_child(
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
            let vm_service_impl = tddy_vm::VmServiceImpl::new(
                Arc::clone(&vm_manager),
                vm_user_resolver,
                task_registry,
            );
            let vm_server = tddy_service::VmServiceServer::new(vm_service_impl);
            rpc_entries.push(tddy_rpc::ServiceEntry {
                name: "vm.VmService",
                service: Arc::new(vm_server) as Arc<dyn tddy_rpc::RpcService>,
            });

            // Screen sharing service — vault management + VNC/RDP bridge spawning.
            // Wire `sessions_base` with the daemon's resolved `tddy_data_dir` so vaults live
            // under the config-only tddy home (config → profile default → `$HOME/.tddy`),
            // matching `sessions_base_resolver` above — not a statically-derived `$HOME/.tddy`.
            let ss_key_cache: tddy_daemon::screen_sharing_service::ScreenSharingKeyCache =
                Arc::new(Mutex::new(HashMap::new()));
            let ss_sessions_base: tddy_daemon::screen_sharing_service::SessionsBase = {
                let dd = tddy_data_dir.clone();
                Arc::new(move |user: &str| {
                    tddy_daemon::user_sessions_path::sessions_base_for_user(user, Some(&dd))
                })
            };
            let ss_svc = tddy_daemon::screen_sharing_service::ScreenSharingServiceImpl::new(
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

        // Relay mode: spawn idle-monitor task that fires the shutdown channel on timeout.
        let idle_monitor_task = idle_tx_opt.map(|tx| {
            let tracker = idle_tracker_opt.expect("tx implies tracker");
            tokio::spawn(async move {
                let check_interval = std::time::Duration::from_secs(30);
                loop {
                    tokio::time::sleep(check_interval).await;
                    if tracker.should_shutdown() {
                        log::info!(
                            "relay daemon: idle timeout expired — triggering graceful shutdown"
                        );
                        let _ = tx.send(());
                        return;
                    }
                }
            })
        });

        let inbound_task = if let Some((bot, harness)) = telegram_inbound {
            Some(tokio::spawn(async move {
                if let Err(e) = tddy_daemon::telegram_bot::run_telegram_bot(bot, harness).await {
                    log::warn!(
                        target: "tddy_daemon::telegram_bot",
                        "inbound dispatcher ended: {e:#}"
                    );
                }
            }))
        } else {
            None
        };

        let service_name_strs: Vec<&str> = rpc_entries.iter().map(|e| e.name).collect();
        rpc_entries.push(tddy_service::reflection_entry_from(&service_name_strs));

        // If LiveKit is configured with a common room, serve the daemon's RPC services via LiveKit
        // data channel so the RPC Playground can discover and invoke them without HTTP streaming issues.
        if let Some(lk) = config.livekit.as_ref() {
            let cr = lk
                .common_room
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let url_ok = lk.url.as_deref().map(str::trim).filter(|s| !s.is_empty());
            let key_ok = lk
                .api_key
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let sec_ok = lk
                .api_secret
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if let (Some(common_room_name), Some(url_str), Some(key_str), Some(sec_str)) =
                (cr, url_ok, key_ok, sec_ok)
            {
                let livekit_entries: Vec<tddy_rpc::ServiceEntry> = rpc_entries
                    .iter()
                    .map(|e| tddy_rpc::ServiceEntry {
                        name: e.name,
                        service: e.service.clone(),
                    })
                    .collect();
                let lk_multi = tddy_rpc::MultiRpcService::new(livekit_entries);
                let local_id =
                    tddy_daemon::livekit_peer_discovery::local_instance_id_for_config(&config);
                let rpc_identity = format!("daemon-{local_id}");
                let token_gen = tddy_livekit::TokenGenerator::new(
                    key_str.to_string(),
                    sec_str.to_string(),
                    common_room_name.to_string(),
                    rpc_identity,
                    std::time::Duration::from_secs(tddy_livekit::DEFAULT_LIVEKIT_JWT_TTL_SECS),
                );
                let url_owned = url_str.to_string();
                let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
                tokio::spawn(async move {
                    tddy_livekit::LiveKitParticipant::run_with_reconnect(
                        &url_owned,
                        &token_gen,
                        lk_multi,
                        Default::default(),
                        shutdown,
                        None,
                        None,
                    )
                    .await;
                });
            }
        }

        // Spawn a task that SIGTERMs claude-cli sessions as soon as the daemon receives
        // SIGTERM, independent of how long the HTTP server takes to drain open connections.
        // This prevents orphaned Claude processes when systemd escalates to SIGKILL.
        let kill_on_signal_manager = Arc::clone(&shared_claude_cli_manager);
        let _kill_on_signal_task = tokio::spawn(async move {
            #[cfg(unix)]
            {
                if let Ok(mut sig) =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                {
                    sig.recv().await;
                    log::info!(
                        target: "tddy_daemon",
                        "SIGTERM received — killing all claude-cli sessions"
                    );
                    kill_on_signal_manager.kill_all().await;
                }
            }
        });

        let daemon_instance_id =
            tddy_daemon::livekit_peer_discovery::local_instance_id_for_config(&config);
        let res = tddy_daemon::server::run_server(
            host.as_str(),
            port,
            bundle_path,
            rpc_entries,
            livekit_url,
            common_room,
            daemon_instance_id,
            allowed_agents,
            web_debug,
            lifecycle_telegram,
            idle_rx_opt, // Some(rx) in relay mode; None otherwise
        )
        .await;

        // Also call kill_all after the server finishes (covers graceful ctrl-c shutdown
        // and any sessions started while the first kill_all was already running).
        shared_claude_cli_manager.kill_all().await;

        if let Some(t) = inbound_task {
            t.abort();
        }
        if let Some(m) = idle_monitor_task {
            m.abort();
        }
        res
    })
}
