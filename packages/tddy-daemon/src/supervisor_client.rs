//! Choosing and reaching the privileged spawn backend.
//!
//! On a supervised host the daemon owns no privilege at all: it asks `tddy-supervisor` to spawn
//! sessions, clone repos and build sandbox jails. On an unsupervised host it keeps the forked
//! spawn worker (`crate::spawn_worker`) and whatever its own systemd unit grants it.
//!
//! Which of the two applies is decided from config alone, before any I/O, so the decision is
//! testable and cannot change under a transient connection failure.

use std::path::{Path, PathBuf};

use anyhow::Context;
use tddy_supervisor::SupervisorClient;

use crate::config::DaemonConfig;

/// Which mechanism this daemon uses to spawn processes it may not spawn itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnBackendChoice {
    /// Delegate to `tddy-supervisor` over this socket.
    Supervisor { socket_path: PathBuf },
    /// No supervisor configured: fork a spawn worker before tokio starts, as today.
    ForkedWorker,
}

/// Decide the spawn backend from configuration. Pure — performs no I/O.
pub fn spawn_backend_choice(config: &DaemonConfig) -> SpawnBackendChoice {
    match &config.supervisor {
        Some(supervisor) => SpawnBackendChoice::Supervisor {
            socket_path: supervisor.socket_path.clone(),
        },
        None => SpawnBackendChoice::ForkedWorker,
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
/// spawn anything itself — see `SupervisorClientConfig`. The error names the socket, because the
/// operator's next question is always which path the daemon was looking at.
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
