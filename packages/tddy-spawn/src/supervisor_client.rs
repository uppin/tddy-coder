//! Choosing and reaching the privileged spawn backend.
//!
//! On a supervised host the daemon owns no privilege at all: it asks `tddy-supervisor` to spawn
//! sessions, clone repos and build sandbox jails. On an unsupervised host it keeps the forked
//! spawn worker (`crate::spawn_worker`) and whatever its own systemd unit grants it.
//!
//! Which of the two applies is decided from how the process was started, before any I/O, so the
//! decision is testable and cannot change under a transient connection failure.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::Context;
use tddy_supervisor::spawn_broker::SUPERVISOR_SOCKET_ENV;
use tddy_supervisor::SupervisorClient;

/// Which mechanism this daemon uses to spawn processes it may not spawn itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnBackendChoice {
    /// Delegate to `tddy-supervisor` over this socket.
    Supervisor { socket_path: PathBuf },
    /// Not started by a supervisor: fork a spawn worker before tokio starts.
    ForkedWorker,
}

impl SpawnBackendChoice {
    /// Decide the spawn backend from how this process was started: `Supervisor` when
    /// `tddy-supervisor` told it its socket ([`SUPERVISOR_SOCKET_ENV`]), `ForkedWorker` otherwise.
    /// `env` reads one variable (`std::env::var_os` in production), so the decision is testable
    /// without touching process-wide state.
    pub fn from_environment(env: impl Fn(&str) -> Option<OsString>) -> SpawnBackendChoice {
        match env(SUPERVISOR_SOCKET_ENV) {
            Some(socket_path) => SpawnBackendChoice::Supervisor {
                socket_path: PathBuf::from(socket_path),
            },
            None => SpawnBackendChoice::ForkedWorker,
        }
    }

    /// [`Self::from_environment`], plus the variables the caller must then remove from its own
    /// environment. The socket is addressed to this process only: a child the daemon spawns itself
    /// (git, a hook, a forked worker's session) must not inherit it and mistake itself for a
    /// supervised service. The caller removes them while it is still single-threaded, where
    /// `std::env::remove_var` is sound.
    pub fn adopt_from_environment(env: impl Fn(&str) -> Option<OsString>) -> AdoptedSpawnBackend {
        let choice = SpawnBackendChoice::from_environment(&env);
        let consumed_env = match choice {
            SpawnBackendChoice::Supervisor { .. } => vec![SUPERVISOR_SOCKET_ENV],
            SpawnBackendChoice::ForkedWorker => Vec::new(),
        };
        AdoptedSpawnBackend {
            choice,
            consumed_env,
        }
    }
}

/// The spawn backend a process adopted, and the environment variables it consumed doing so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptedSpawnBackend {
    pub choice: SpawnBackendChoice,
    /// Variables to remove from the process environment before any thread starts.
    pub consumed_env: Vec<&'static str>,
}

impl AdoptedSpawnBackend {
    /// Remove [`Self::consumed_env`] from this process's environment and return the choice.
    /// Call only while the process is single-threaded.
    pub fn consume_from_process_environment(self) -> SpawnBackendChoice {
        for key in &self.consumed_env {
            std::env::remove_var(key);
        }
        self.choice
    }
}

/// Fork the pre-tokio spawn worker, unless the supervisor is what spawns on this host.
///
/// The worker exists only because `fork` from a multi-threaded process can deadlock, so it has to be
/// created before tokio starts. A supervised daemon spawns nothing itself, so it forks nothing —
/// there is no worker to fall back to, which is the point: no code path can quietly downgrade a
/// session isolated as its own user to one running as the daemon.
#[cfg(unix)]
pub fn spawn_worker_for(
    choice: &SpawnBackendChoice,
) -> anyhow::Result<Option<(crate::spawn_worker::SpawnClient, libc::pid_t)>> {
    match choice {
        SpawnBackendChoice::Supervisor { socket_path } => {
            log::info!(
                "spawn backend: tddy-supervisor at {} (no spawn worker forked)",
                socket_path.display()
            );
            Ok(None)
        }
        SpawnBackendChoice::ForkedWorker => crate::spawn_worker::fork_spawn_worker(),
    }
}

#[cfg(not(unix))]
pub fn spawn_worker_for(
    _choice: &SpawnBackendChoice,
) -> anyhow::Result<Option<(crate::spawn_worker::SpawnClient, i32)>> {
    crate::spawn_worker::fork_spawn_worker()
}

/// Connect to the supervisor.
///
/// An unreachable supervisor is a hard error. The caller must fail the operation rather than
/// spawn anything itself — see [`SpawnBackendChoice::Supervisor`]. The error names the socket,
/// because the operator's next question is always which path the daemon was looking at.
pub async fn connect_supervisor(socket_path: &Path) -> anyhow::Result<SupervisorClient> {
    SupervisorClient::connect(socket_path)
        .await
        .with_context(|| {
            format!(
                "could not reach tddy-supervisor at {}",
                socket_path.display()
            )
        })
}
