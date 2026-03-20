//! tddy-daemon — multi-user daemon for tddy-* tools.
//!
//! Runs as root process. Handles GitHub auth, user mapping, session discovery,
//! and spawns tddy-* processes as the target OS user.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "tddy-daemon")]
#[command(about = "Multi-user daemon for tddy-* tools")]
struct Args {
    /// Path to config file (YAML)
    #[arg(short, long, env = "TDDY_DAEMON_CONFIG")]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config_path = args
        .config
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--config is required"))?;

    let config = tddy_daemon::config::DaemonConfig::load(config_path)?;
    log::info!("tddy-daemon loaded config from {}", config_path.display());

    let port = config
        .listen
        .web_port
        .ok_or_else(|| anyhow::anyhow!("config.listen.web_port is required"))?;
    let host = config.listen.web_host.as_deref().unwrap_or("0.0.0.0");
    let bundle_path = config
        .web_bundle_path
        .clone()
        .ok_or_else(|| anyhow::anyhow!("config.web_bundle_path is required"))?;

    let livekit_url = config
        .livekit
        .as_ref()
        .and_then(|l| l.public_url.clone())
        .or_else(|| config.livekit.as_ref().and_then(|l| l.url.clone()));

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
        );
        let connection_server = tddy_service::ConnectionServiceServer::new(connection_impl);
        rpc_entries.push(tddy_rpc::ServiceEntry {
            name: "connection.ConnectionService",
            service: Arc::new(connection_server) as Arc<dyn tddy_rpc::RpcService>,
        });
    }

    tddy_daemon::server::run_server(host, port, bundle_path, rpc_entries, livekit_url).await
}
