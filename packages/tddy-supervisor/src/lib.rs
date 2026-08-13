//! `tddy-supervisor` — a small root-run process that keeps the privileged surface of the tddy
//! stack out of `tddy-daemon`.
//!
//! It does two jobs:
//!
//! 1. **Mini-init.** It starts the services declared in its root-owned config (`tddy-daemon`
//!    first among them), drops to their declared unprivileged user, reaps them, restarts them
//!    with backoff, and shuts them down on `SIGTERM`.
//! 2. **Privileged broker.** It serves a narrow RPC surface on a unix socket for the four
//!    operations that genuinely need privilege — controlling declared services, cgroup v2 scope
//!    lifecycle, spawning sessions as other OS users, and sandbox namespace/mount setup — with
//!    every request authorized by peer credentials and validated against root-owned policy.
//!
//! See `docs/ft/supervisor/1-WIP/PRD-2026-08-02-tddy-supervisor.md`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::signal::unix::{signal, SignalKind};

pub mod proto {
    /// `SupervisorService`: the privileged surface served on the root-owned unix socket. RpcService
    /// flavor only — it rides tddy-rpc's frame codec over AF_UNIX, never gRPC.
    #[allow(unused_imports, unused_variables)]
    pub mod supervisor {
        include!(concat!(env!("OUT_DIR"), "/supervisor.rs"));
    }
}

pub mod authz;
pub mod cgroup_broker;
pub mod client;
pub mod config;
pub mod error;
pub mod policy;
pub mod protocol;
pub mod reaper;
pub mod request;
pub mod restart;
pub mod server;
pub mod service;
pub mod services;
pub mod signals;
pub mod socket;
pub mod spawn_broker;
pub mod supervisor;

#[cfg(test)]
mod test_util;

pub use authz::{Authorizer, PeerIdentity};
pub use cgroup_broker::CgroupBroker;
pub use client::SupervisorClient;
pub use config::{
    CgroupPolicy, ManagedService, RestartPolicy, SocketConfig, SpawnPolicy, SupervisorConfig,
};
pub use error::{ConfigError, SupervisorError};
pub use request::{
    AppliedLimits, CreateScopeRequest, RequestedLimits, SandboxMount, ScopeHandle, SessionState,
    SessionStatus, SpawnSandboxRequest, SpawnSessionRequest, SpawnedProcess,
};
pub use restart::{BackoffState, RestartDecision};
pub use service::{ServiceState, ServiceStatus};
pub use services::{ExitOutcome, ServiceRuntime};
pub use socket::{SocketSource, SD_LISTEN_FDS_START};
pub use supervisor::Supervisor;

/// Run the supervisor: load config, start declared services, serve the privileged socket, and
/// return once a shutdown signal has been handled.
pub async fn run(config_path: &Path) -> anyhow::Result<()> {
    let config = Arc::new(
        config::SupervisorConfig::load(config_path)
            .map_err(|error| anyhow::anyhow!("{}: {error}", config_path.display()))?,
    );

    // Signal streams first: a service that dies the instant it is exec'd would otherwise raise its
    // `SIGCHLD` before anything was listening for it, and the supervisor would wait forever for a
    // notification it had already missed.
    let mut child_exited = signal(SignalKind::child())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;

    // Every fork in the process goes through this one thread, and it must outlive every child it
    // creates — see `spawn_broker::ForkBroker`.
    let forks = Arc::new(spawn_broker::ForkBroker::start()?);
    let supervisor = Supervisor::new(&config, Arc::clone(&forks))?;
    let cgroups = CgroupBroker::new(
        cgroup_broker::resolve_cgroup_base(&config.cgroup)?,
        config.cgroup.clone(),
    );
    // Before any child exists, and before the first scope is asked for. The supervisor has to leave
    // the cgroup it carves scopes out of before cgroup v2 will let it delegate controllers there, and
    // a child forked afterwards is created in the leaf the supervisor moved into rather than in the
    // base — which is what keeps the base free of processes for the rest of the run.
    cgroups.prepare_delegated_subtree(std::process::id())?;
    let surface = Arc::new(server::PrivilegedSurface::new(
        Arc::clone(&config),
        Arc::clone(&supervisor),
        cgroups,
    ));

    // Bind before the first service starts. The socket appearing is what tells the rest of the
    // host that this supervisor is up, and binding first means a failure to bind cannot leave
    // orphaned services behind.
    let listener = server::bind_privileged_listener(&config.socket)?;

    let reaper = tokio::spawn({
        let supervisor = Arc::clone(&supervisor);
        async move {
            while child_exited.recv().await.is_some() {
                supervisor.reap().await;
            }
        }
    });

    supervisor.start_declared_services().await;
    let server = tokio::spawn(server::serve(listener, Arc::clone(&surface)));

    tokio::select! {
        _ = terminate.recv() => log::info!(target: "tddy_supervisor", "SIGTERM received"),
        _ = interrupt.recv() => log::info!(target: "tddy_supervisor", "SIGINT received"),
    }

    // Stop accepting first: a request that arrived mid-shutdown could only ask for work on
    // processes that are already being torn down.
    server.abort();
    supervisor
        .shutdown(Duration::from_secs(config.shutdown_grace_secs))
        .await;
    reaper.abort();
    Ok(())
}
