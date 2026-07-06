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

    // Fork spawn worker before tokio — fork() from multi-threaded process can deadlock.
    let spawn_client = tddy_daemon::spawn_worker::fork_spawn_worker()?;
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

    let allowed_agents: Vec<tddy_coder::web_server::ClientAllowedAgent> =
        tddy_daemon::agent_list_mapping::agent_allowlist_rows(&config)
            .into_iter()
            .map(|row| tddy_coder::web_server::ClientAllowedAgent {
                id: row.id,
                label: row.display_label,
            })
            .collect();

    let auth_result = tddy_daemon::auth::build_auth_entries(&config, host.as_str(), port);
    let mut rpc_entries = auth_result.entries;

    if let Some(ref lk) = config.livekit {
        if let (Some(api_key), Some(api_secret)) = (&lk.api_key, &lk.api_secret) {
            let token_generator = std::sync::Arc::new(tddy_livekit::TokenGenerator::new(
                api_key.clone(),
                api_secret.clone(),
                "daemon".to_string(),
                "token-provider".to_string(),
                std::time::Duration::from_secs(tddy_livekit::DEFAULT_LIVEKIT_JWT_TTL_SECS),
            ));
            let token_provider = tddy_daemon::token_provider::LiveKitTokenProvider(token_generator);
            let token_service_impl = tddy_service::TokenServiceImpl::new(token_provider);
            let token_server = tddy_service::TokenServiceServer::new(token_service_impl);
            rpc_entries.push(tddy_rpc::ServiceEntry {
                name: "token.TokenService",
                service: std::sync::Arc::new(token_server)
                    as std::sync::Arc<dyn tddy_rpc::RpcService>,
            });
        }
    }

    // Create one shared ClaudeCliSessionManager — injected into both the Telegram spawn path and
    // ConnectionServiceImpl so that Telegram-launched sessions are attachable via the terminal RPCs.
    let shared_claude_cli_manager =
        Arc::new(tddy_daemon::cli_session_manager::CliSessionManager::new());

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
                        ),
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
            let mut connection_impl = tddy_daemon::connection_service::ConnectionServiceImpl::new(
                config.clone(),
                sessions_base_resolver,
                tddy_data_dir.clone(),
                user_resolver,
                spawn_client,
                livekit_discovery,
                telegram_hooks.clone(),
                Arc::clone(&shared_claude_cli_manager),
            );
            if let Some(ref tracker) = idle_tracker_opt {
                connection_impl = connection_impl.with_idle_tracker(tracker.clone());
            }
            // Get the shared TaskRegistry before moving connection_impl into the server.
            let task_registry = connection_impl.task_registry();
            let connection_server = tddy_service::ConnectionServiceServer::new(connection_impl);
            rpc_entries.push(tddy_rpc::ServiceEntry {
                name: "connection.ConnectionService",
                service: Arc::new(connection_server) as Arc<dyn tddy_rpc::RpcService>,
            });

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
