//! tddy-daemon — multi-user daemon for tddy-* tools.
//!
//! Runs as root process. Handles GitHub auth, user mapping, session discovery,
//! and spawns tddy-* processes as the target OS user.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;

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

    if let Some(user_resolver) = auth_result.user_resolver {
        let connection_impl = tddy_daemon::connection_service::ConnectionServiceImpl::new(
            config.clone(),
            Arc::new(tddy_daemon::user_sessions_path::sessions_base_for_user),
            user_resolver,
            spawn_client,
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
    rt.block_on(tddy_daemon::server::run_server(
        host,
        port,
        bundle_path,
        rpc_entries,
        livekit_url,
        common_room,
    ))
}
