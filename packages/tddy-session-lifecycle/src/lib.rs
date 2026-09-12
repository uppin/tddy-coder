//! A session's whole life: listing, starting, connecting, resuming, signalling and deleting.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 9, serving `session.SessionService` — family C.

mod handler;
mod service;

pub use handler::{SessionHandler, SessionStartEventStream};
pub use service::{build_session_entry, SessionServiceImpl};

/// Why a session operation could not be completed.
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
    task_registry: tddy_task::TaskRegistry,
}

impl CliSessionManager {
    /// Build a manager, creating the `TaskRegistry` every long-running task is tracked in.
    pub fn new() -> Self {
        Self {
            task_registry: tddy_task::TaskRegistry::new(),
        }
    }

    /// The registry this manager created.
    pub fn task_registry(&self) -> tddy_task::TaskRegistry {
        self.task_registry.clone()
    }

    /// Kill every session this manager started, for shutdown.
    pub async fn kill_all(&self) {
        let tasks = self.task_registry.list().await;
        for task in tasks {
            let _ = self.task_registry.cancel_task(&task.id).await;
        }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn names_the_service_family_c_moves_to() {
        struct Noop;
        #[async_trait::async_trait]
        impl SessionHandler for Noop {
            async fn list_sessions(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::session::ListSessionsRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::session::ListSessionsResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn start_session(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::session::StartSessionRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::session::StartSessionResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn stream_start_session(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::session::StartSessionRequest>,
            ) -> Result<tddy_rpc::Response<SessionStartEventStream>, tddy_rpc::Status> {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn connect_session(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::session::ConnectSessionRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::session::ConnectSessionResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn resume_session(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::session::ResumeSessionRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::session::ResumeSessionResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn signal_session(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::session::SignalSessionRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::session::SignalSessionResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn delete_session(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::session::DeleteSessionRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::session::DeleteSessionResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
            async fn get_worktree_snapshot(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::session::GetWorktreeSnapshotRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::session::GetWorktreeSnapshotResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("test stub"))
            }
        }

        let entry = build_session_entry(SessionServiceImpl::new(Arc::new(Noop)));
        assert_eq!(entry.name, "session.SessionService");
    }

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

    #[tokio::test]
    async fn hands_out_the_task_registry_it_created_itself() {
        let manager = CliSessionManager::new();
        let registry = task_registry(&manager);
        assert!(
            registry
                .get(&tddy_task::TaskId("never-registered".to_string()))
                .await
                .is_none(),
            "a registry handed straight from its creator starts empty"
        );
    }
}
