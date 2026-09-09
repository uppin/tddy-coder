//! Spawning a session's process: the unprivileged spawn worker, and delegation to the privileged
//! supervisor when one is brokering.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 3, as a **separate crate from
//! `tddy-daemon-sandbox`** — the two share no `crate::` edge and their dependency sets are disjoint
//! (`tddy-supervisor` here, six `tddy-sandbox*` crates there). Confinement and privileged fork are
//! different concerns.
//!
//! # The fork happens before tokio starts
//!
//! `main.rs` calls [`spawn_backend_choice`] and then [`spawn_worker_for`] **before** it builds the
//! tokio runtime. That ordering is not incidental: forking a process that already has a
//! multi-threaded runtime is unsound, because only the calling thread survives the fork and any
//! lock the others held is held forever. Anything moved here has to preserve it, so the worker's
//! constructor stays synchronous and takes no runtime handle.

/// Which mechanism will spawn this daemon's session processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnBackend {
    /// Fork an unprivileged worker in this process's own user.
    Unprivileged,
    /// Ask the privileged supervisor, which can setuid and delegate cgroups.
    Supervisor,
}

/// A handle on the forked worker, plus the pid the daemon reaps.
#[derive(Debug)]
pub struct SpawnClient {
    // TODO(sandbox-spawn-services): implement
}

/// Why a worker could not be forked or reached.
#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error("the spawn worker could not be forked: {reason}")]
    ForkFailed { reason: String },
    #[error("the supervisor is configured but not reachable at {path}")]
    SupervisorUnreachable { path: String },
}

/// Decide which backend to use from configuration.
///
/// Reading configuration rather than probing is deliberate: a daemon told to use the supervisor and
/// unable to reach it must fail loudly, not quietly spawn unprivileged sessions an operator believes
/// are brokered.
pub fn spawn_backend_choice(_supervisor_socket: Option<&str>) -> SpawnBackend {
    // TODO(sandbox-spawn-services): implement
    unimplemented!("spawn_backend_choice")
}

/// Fork the worker for a chosen backend. **Must be called before the tokio runtime is built.**
pub fn spawn_worker_for(_backend: SpawnBackend) -> Result<(SpawnClient, i32), SpawnError> {
    // TODO(sandbox-spawn-services): implement
    unimplemented!("spawn_worker_for")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chooses_the_supervisor_when_one_is_configured() {
        // Given a configured supervisor socket
        // When
        let backend = spawn_backend_choice(Some("/run/tddy/supervisor.sock"));

        // Then
        assert_eq!(backend, SpawnBackend::Supervisor);
    }

    /// No socket configured means no supervisor — not "probe and hope".
    #[test]
    fn chooses_the_unprivileged_worker_when_no_supervisor_is_configured() {
        // When
        let backend = spawn_backend_choice(None);

        // Then
        assert_eq!(backend, SpawnBackend::Unprivileged);
    }

    /// A daemon told to use the supervisor and unable to reach it must say so, because an operator
    /// believes those sessions are brokered.
    #[test]
    fn refuses_to_fall_back_when_a_configured_supervisor_is_unreachable() {
        // When
        let outcome = spawn_worker_for(SpawnBackend::Supervisor);

        // Then
        assert!(
            matches!(outcome, Err(SpawnError::SupervisorUnreachable { .. })),
            "an unreachable supervisor is an error, never a silent unprivileged spawn"
        );
    }
}
