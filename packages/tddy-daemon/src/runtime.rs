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

mod build_context;
mod env;
mod identity;
mod integrations;
mod mount;
mod mount_accounts;
mod mount_entries;
mod mount_host;
mod mount_socket;
mod tasks;
mod telegram_wiring;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tddy_service::proto::daemon_config::DaemonConfigServiceServer;
use tddy_service::proto::host::HostServiceTonicAdapter;
use tddy_service::proto::worktree::WorktreeServiceTonicAdapter;
use tokio::sync::Mutex;

use crate::config::DaemonConfig;
use crate::daemon_config_service::{CommonRoomSupervisor, DaemonConfigServiceImpl};
use crate::runtime::tasks::CredentialSyncPublishWatch;
use crate::runtime::tasks::DaemonChildren;
use crate::runtime::tasks::LocalSocketTransport;
use crate::runtime::tasks::OauthLoopbackTunnel;
use tddy_daemon_livekit::common_room_supervisor::{
    cloned_entries, DaemonCommonRoomConnector, PeerDiscoveryHandles,
};
use tddy_telegram_control::telegram_notifier::TelegramSender;

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
    /// either because `tddy-supervisor` does it (see [`tddy_spawn::supervisor_client`]) or because the
    /// host has no worker to offer.
    pub spawn_client: Option<(tddy_spawn::spawn_worker::SpawnClient, i32)>,
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
        spawn_client: Option<(tddy_spawn::spawn_worker::SpawnClient, i32)>,
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
    pub cli_sessions: Arc<tddy_session_lifecycle::cli_session_manager::CliSessionManager>,
    /// The session host this runtime assembled, or `None` for a daemon built without
    /// authentication — which serves no session RPCs and therefore has no session children.
    ///
    /// Held so the host can reach [`DaemonChildren`]: the CLI sessions *and* the workspace jails,
    /// the second of which nothing outside this process would ever stop.
    pub session_host: Option<Arc<tddy_session_lifecycle::connection_service::DaemonSessionHost>>,
    /// The Telegram chat that gets the "started"/"stopped" messages, when a bot is configured.
    pub lifecycle_telegram: Option<(DaemonConfig, Arc<dyn TelegramSender + Send + Sync>)>,
    /// Relay mode: fires once the idle timeout expires, for a server that shuts down gracefully.
    /// The monitor that fires it is started by [`RuntimeTasks::spawn`].
    pub relay_shutdown: Option<tokio::sync::oneshot::Receiver<()>>,
    /// The index daemon this runtime manages, when an `index_daemon:` section asked for one.
    /// `None` for every deployment that did not.
    ///
    /// Handed to the host as well as to [`RuntimeTasks`] because the host has to *stop* it: the
    /// index daemon holds a rust-analyzer per workspace root, and leaving it behind is the defect
    /// `docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md` records
    /// against the sandbox path.
    pub index_daemon: Option<crate::index_daemon::IndexDaemonRegistry>,
    /// Everything this runtime needs running but has not started: see [`RuntimeTasks`].
    pub tasks: RuntimeTasks,
    /// The credential vaults the auth and accounts services share, when `auth_storage` is set.
    /// Private, read through [`DaemonRuntime::credential_vaults`].
    credential_vaults: Option<Arc<tddy_daemon_auth::SessionVaults>>,
}

impl DaemonRuntime {
    /// The names of the services this runtime hosts, in registration order.
    pub fn service_names(&self) -> Vec<&str> {
        self.entries.iter().map(|entry| entry.name).collect()
    }

    /// The very vaults this runtime's services hold open — one handle, not a second instance over
    /// the same directory, whose in-memory open state would be its own. `None` without
    /// `auth_storage`, where nothing keeps a credential.
    pub fn credential_vaults(&self) -> Option<&Arc<tddy_daemon_auth::SessionVaults>> {
        self.credential_vaults.as_ref()
    }

    /// The child processes this daemon spawned, as a handle a signal task can own.
    ///
    /// Cloneable and independent of the runtime, because the shutdown path needs it in two places
    /// at once: a task waiting on SIGTERM, and the graceful return from `run_server`.
    pub fn children(&self) -> DaemonChildren {
        DaemonChildren {
            cli_sessions: Arc::clone(&self.cli_sessions),
            session_host: self.session_host.clone(),
            index_daemon: self.index_daemon.clone(),
        }
    }
}

use crate::runtime::build_context::BuildContext;
use crate::runtime::env::apply_env_overrides;
use crate::runtime::env::tddy_data_dir_for;
use crate::runtime::identity::Identity;
pub use crate::runtime::tasks::{
    spawn_common_room_discovery_task, RuntimeTaskHandles, RuntimeTasks,
};

/// How this deployment admits its first login, when it may enrol one at all.
///
/// Only a desktop enrols: an application embedding the daemon, started from a config file it can
/// write the row back into, mapping the login to the account the application runs as. A server or
/// systemd daemon's `users:` is its installer's to write, so it gets no admission — an empty map
/// there refuses every caller's RPCs exactly as it always has.
///
/// An embedding host that serves sign-in to a `users:` mapping nobody, but names no config file,
/// is refused here: its operator's first login could never be written down, so every sign-in would
/// appear to work and then be refused by every RPC. It must say where the daemon was loaded from.
///
/// `config.users` is the holder every service built from `config` authorizes through, so the row
/// enrolled here is the row they all see.
fn first_login_enrolment(
    config: &DaemonConfig,
    options: &RuntimeOptions,
) -> anyhow::Result<Option<Arc<dyn tddy_github::LoginAdmission>>> {
    if options.host != RuntimeHost::Embedded {
        return Ok(None);
    }
    let Some(config_path) = &options.config_path else {
        let serves_sign_in = tddy_daemon_auth::auth::github_auth_flow(config).is_some();
        if serves_sign_in && config.users.is_empty() {
            anyhow::bail!(
                "this embedded daemon serves GitHub sign-in and maps no users, but was given no \
                 config file to enrol its first login into; pass the file it was loaded from \
                 (RuntimeOptions::with_config_path)"
            );
        }
        return Ok(None);
    };
    let os_user = this_process_os_user().ok_or_else(|| {
        anyhow::anyhow!(
            "cannot resolve the OS user this desktop runs as, which its first GitHub login is \
             enrolled as"
        )
    })?;
    Ok(Some(Arc::new(tddy_daemon_auth::FirstLoginEnrolment::new(
        config.users.clone(),
        config_path.clone(),
        os_user,
    ))))
}

/// The OS username this process runs as.
#[cfg(unix)]
fn this_process_os_user() -> Option<String> {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    tddy_session_lifecycle::user_sessions_path::username_for_uid(unsafe { libc::getuid() })
}

#[cfg(not(unix))]
fn this_process_os_user() -> Option<String> {
    None
}

/// Assemble the daemon described by `config` for the host named in `options`.
pub async fn build(
    mut config: DaemonConfig,
    options: RuntimeOptions,
) -> anyhow::Result<DaemonRuntime> {
    // Apply env overrides (e.g. from .env loaded by web-dev) before anything reads the config.
    apply_env_overrides(&mut config);

    let identity = Identity::assemble(&config, &options)?;
    let mut context = BuildContext::new(config, options, identity);
    context.mount_session_services().await?;
    context.mount_daemon_settings();
    context.serve_common_room();
    Ok(context.into_runtime())
}

pub use crate::runtime::telegram_wiring::split_room_hosting;

#[cfg(test)]
mod data_dir {
    use super::*;
    use crate::runtime::env::data_dir_from;

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
