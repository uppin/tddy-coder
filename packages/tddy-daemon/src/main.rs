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

    let port = config
        .listen
        .web_port
        .ok_or_else(|| anyhow::anyhow!("config.listen.web_port is required"))?;
    let host = config.listen.web_host.as_deref().unwrap_or("0.0.0.0");
    log::info!("tddy-daemon listening on {}:{}", host, port);
    let bundle_path = config
        .web_bundle_path
        .clone()
        .ok_or_else(|| anyhow::anyhow!("config.web_bundle_path is required"))?;

    let livekit_url = config
        .livekit
        .as_ref()
        .and_then(|l| l.public_url.clone())
        .or_else(|| config.livekit.as_ref().and_then(|l| l.url.clone()));

    let common_room = config.livekit.as_ref().and_then(|l| l.common_room.clone());

    let auth_result = tddy_daemon::auth::build_auth_entries(&config, host, port);
    let mut rpc_entries = auth_result.entries;

    if let Some(ref lk) = config.livekit {
        if let (Some(api_key), Some(api_secret)) = (&lk.api_key, &lk.api_secret) {
            let token_generator = std::sync::Arc::new(tddy_livekit::TokenGenerator::new(
                api_key.clone(),
                api_secret.clone(),
                "daemon".to_string(),
                "token-provider".to_string(),
                std::time::Duration::from_secs(120),
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
                let active_elicitation = Arc::new(StdMutex::new(
                    tddy_daemon::active_elicitation::ActiveElicitationCoordinator::new(),
                ));
                let watcher = Arc::new(Mutex::new(
                    tddy_daemon::telegram_notifier::TelegramSessionWatcher::with_elicitation_select_options_and_coordinator(
                        elicitation_select_options.clone(),
                        active_elicitation.clone(),
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
                    tddy_daemon::user_sessions_path::tddy_data_root_matching_child(&user)
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
                            telegram_hooks: Some(hooks.clone()),
                            child_grpc_by_session: Arc::new(StdMutex::new(HashMap::new())),
                            elicitation_select_options: elicitation_select_options.clone(),
                            pending_elicitation_other: Arc::new(StdMutex::new(HashMap::new())),
                        },
                    ));
                    let harness = Arc::new(Mutex::new(
                        tddy_daemon::telegram_session_control::TelegramSessionControlHarness::with_workflow_spawn(
                            tg.chat_ids.clone(),
                            sessions_base,
                            teloxide_sender,
                            workflow_spawn,
                            Some(active_elicitation),
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

    if let Some(user_resolver) = auth_result.user_resolver {
        let connection_impl = tddy_daemon::connection_service::ConnectionServiceImpl::new(
            config.clone(),
            Arc::new(tddy_daemon::user_sessions_path::sessions_base_for_user),
            user_resolver,
            spawn_client,
            Some(Arc::new(tddy_daemon::multi_host::StubEligibleDaemonSource)),
            telegram_hooks.clone(),
        );
        let connection_server = tddy_service::ConnectionServiceServer::new(connection_impl);
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "connection.ConnectionService",
            service: Arc::new(connection_server) as Arc<dyn tddy_rpc::RpcService>,
        });
    }

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
    rt.block_on(async {
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

        let res = tddy_daemon::server::run_server(
            host,
            port,
            bundle_path,
            rpc_entries,
            livekit_url,
            common_room,
            lifecycle_telegram,
        )
        .await;

        if let Some(t) = inbound_task {
            t.abort();
        }
        res
    })
}
