//! tddy-daemon — multi-user daemon for tddy-* tools.
//!
//! Runs as root process. Handles GitHub auth, user mapping, session discovery,
//! and spawns tddy-* processes as the target OS user.
//!
//! The bootstrap itself lives in [`tddy_daemon::runtime`]: this binary is one of its hosts, and a
//! process that embeds the daemon is another. What is left here is what only a binary owns — the
//! argument parsing, the pre-tokio fork of the spawn worker, the HTTP listener, and the signals a
//! service manager sends it.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tddy_daemon::runtime::{self, RuntimeOptions};

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

    let config = tddy_daemon::config::DaemonConfig::load(config_path)?;

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

    // Scope git's ssh command to this daemon — applied to remote fetches only, without polluting the
    // process environment or global git config. See DaemonConfig::git / GitConfig::ssh_command.
    tddy_core::set_git_ssh_command(config.git.as_ref().and_then(|g| g.ssh_command.clone()));

    // Validated before anything is assembled, so a missing port or web bundle fails at startup
    // rather than after the roster is built. Neither field is environment-overridable, so what the
    // runtime resolves them to is what is checked here.
    let (port, bundle_path_opt) = tddy_daemon::startup::startup_config_check(&config, args.relay)?;
    // In relay mode bundle_path is None; in non-relay mode startup_config_check already
    // guaranteed it is Some (returning Err otherwise). Unwrap is safe for non-relay path.
    let bundle_path = bundle_path_opt.unwrap_or_else(|| PathBuf::from(""));

    // In relay mode, the daemon shuts itself down after this much idleness.
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
    rt.block_on(async move {
        let daemon = runtime::build(
            config,
            RuntimeOptions::for_binary()
                .with_config_path(Some(config_path.to_path_buf()))
                .with_relay_idle_timeout(relay_idle_timeout)
                .with_spawn_worker(spawn_client),
        )
        .await?;

        let host = daemon
            .config
            .listen
            .web_host
            .clone()
            .unwrap_or_else(|| "0.0.0.0".to_string());
        log::info!("tddy-daemon listening on {}:{}", host, port);

        let livekit_url = daemon
            .config
            .livekit
            .as_ref()
            .and_then(|l| l.public_url.clone())
            .or_else(|| daemon.config.livekit.as_ref().and_then(|l| l.url.clone()));
        let common_room = daemon
            .config
            .livekit
            .as_ref()
            .and_then(|l| l.common_room.clone());
        // Browser DEBUG mask (debug-package namespaces) exposed at /api/config; see DaemonConfig::debug.
        let web_debug = daemon.config.debug.clone();
        // The startup snapshot the web bundle is served with. Config entries only: assistants are
        // created and deleted while the daemon runs, so no snapshot could stay right about them —
        // `ListAgents` is their live source, and it reads the registry on every call.
        let allowed_agents: Vec<tddy_coder::web_server::ClientAllowedAgent> =
            tddy_daemon::agent_list_mapping::agent_allowlist_rows(&daemon.config, &[])
                .into_iter()
                .map(|row| tddy_coder::web_server::ClientAllowedAgent {
                    id: row.id,
                    label: row.display_label,
                })
                .collect();
        let daemon_instance_id =
            tddy_daemon::livekit_peer_discovery::local_instance_id_for_config(&daemon.config);

        // Start what the runtime assembled but left to its host: the local socket, the common-room
        // participant, peer discovery, the Telegram dispatcher and the background loops.
        let tasks = daemon.tasks.spawn();

        // Spawn a task that SIGTERMs claude-cli sessions as soon as the daemon receives
        // SIGTERM, independent of how long the HTTP server takes to drain open connections.
        // This prevents orphaned Claude processes when systemd escalates to SIGKILL.
        let kill_on_signal_manager = Arc::clone(&daemon.cli_sessions);
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

        let res = tddy_daemon::server::run_server(
            host.as_str(),
            port,
            bundle_path,
            daemon.entries,
            livekit_url,
            common_room,
            daemon_instance_id,
            allowed_agents,
            web_debug,
            daemon.lifecycle_telegram,
            daemon.relay_shutdown, // Some(rx) in relay mode; None otherwise
        )
        .await;

        // Also call kill_all after the server finishes (covers graceful ctrl-c shutdown
        // and any sessions started while the first kill_all was already running).
        daemon.cli_sessions.kill_all().await;

        tasks.abort_all();
        res
    })
}
