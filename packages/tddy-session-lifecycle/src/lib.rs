//! A session's whole life: listing, starting, connecting, resuming, signalling and deleting.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 9, serving `session.SessionService` — family C,
//! 8 methods, and the last family to leave.
//!
//! # This is the family that made the god object a god object
//!
//! `ConnectionServiceImpl`'s 60 fields, its 21 `with_*` builders and its
//! `Weak<ConnectionServiceImpl>` self-handle all existed to serve these eight methods. That is why
//! the eight-node plan could move 73 of 90 methods and still leave the architectural defect it was
//! aimed at: family C stayed, so the aggregation point stayed with it.
//!
//! # It owns the `TaskRegistry`, because it always created it
//!
//! The registry originates in `CliSessionManager` and was only ever *re-exposed* through
//! `ConnectionServiceImpl`, from which five services took it. Dissolving the god object without
//! moving the registry would leave five crates reaching into the daemon for something the daemon
//! does not own. [`task_registry`] is that surface.

use std::sync::Arc;

use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};

/// Why a session operation could not be completed.
///
/// `NoSuchSession` and `NotThisHost` are deliberately distinct. A session this daemon has never
/// heard of is a caller error; a session that exists but belongs to another host is a **routing**
/// fact the caller can act on by asking that host. Collapsing them turns "ask elsewhere" into
/// "this does not exist", which is how a cross-host attach looks like data loss.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("no session {session_id}")]
    NoSuchSession { session_id: String },
    #[error("session {session_id} belongs to {daemon_instance_id}, not this host")]
    NotThisHost {
        session_id: String,
        daemon_instance_id: String,
    },
    #[error("the session could not be started: {reason}")]
    StartFailed { reason: String },
}

/// The CLI process behind a session — the PTY, its lifecycle, and the registry it creates.
pub struct CliSessionManager {
    // TODO(daemon-becomes-wiring): implement
}

impl CliSessionManager {
    /// Build a manager, creating the `TaskRegistry` every long-running task is tracked in.
    pub fn new() -> Self {
        // TODO(daemon-becomes-wiring): implement
        unimplemented!("CliSessionManager::new")
    }

    /// The registry this manager created.
    ///
    /// Five services take a handle on it. It is exposed here rather than from a service impl because
    /// **this is where it is created** — the god object only ever forwarded it.
    pub fn task_registry(&self) -> tddy_task::TaskRegistry {
        // TODO(daemon-becomes-wiring): implement
        unimplemented!("CliSessionManager::task_registry")
    }

    /// Kill every session this manager started, for shutdown.
    pub async fn kill_all(&self) {
        // TODO(daemon-becomes-wiring): implement
        unimplemented!("CliSessionManager::kill_all")
    }
}

impl Default for CliSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// The `TaskRegistry` a daemon's services share, from the crate that creates it.
pub fn task_registry(manager: &CliSessionManager) -> tddy_task::TaskRegistry {
    manager.task_registry()
}

/// The `session.SessionService` entry the daemon's wiring layer registers.
pub fn build_session_entry(
    _sessions_base: SessionsBaseResolver,
    _user_resolver: SessionUserResolver,
    _cli_sessions: Arc<CliSessionManager>,
) -> tddy_rpc::ServiceEntry {
    // TODO(daemon-becomes-wiring): implement
    unimplemented!("build_session_entry")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_service_family_c_moves_to() {
        // Given
        let sessions_base: SessionsBaseResolver = Arc::new(|_| None);
        let user_resolver: SessionUserResolver = Arc::new(|_| None);
        let cli = Arc::new(CliSessionManager::new());

        // When
        let entry = build_session_entry(sessions_base, user_resolver, cli);

        // Then
        assert_eq!(entry.name, "session.SessionService");
    }

    /// A session on another host is a routing fact, not an absence. Collapsing the two turns "ask
    /// that host" into "this does not exist", which is how a cross-host attach reads as data loss.
    #[test]
    fn tells_a_session_on_another_host_apart_from_one_that_does_not_exist() {
        let absent = SessionError::NoSuchSession {
            session_id: "session-a".to_string(),
        };
        let elsewhere = SessionError::NotThisHost {
            session_id: "session-a".to_string(),
            daemon_instance_id: "daemon-7".to_string(),
        };

        assert!(absent.to_string().contains("no session"));
        assert!(elsewhere.to_string().contains("belongs to daemon-7"));
    }

    /// The registry is obtained from the crate that creates it, not forwarded through a service.
    /// This is the assertion that makes the god object's dissolution real rather than nominal:
    /// `TaskRegistry` originates in `CliSessionManager` and was only ever *re-exposed* through
    /// `ConnectionServiceImpl`, from which five services took it.
    #[tokio::test]
    async fn hands_out_the_task_registry_it_created_itself() {
        // Given
        let manager = CliSessionManager::new();

        // When
        let registry = task_registry(&manager);

        // Then — a fresh registry, obtained with no service in the way
        assert!(
            registry
                .get(&tddy_task::TaskId("never-registered".to_string()))
                .await
                .is_none(),
            "a registry handed straight from its creator starts empty"
        );
    }
}
