//! The host as an extension sees it, and the builder that installs one.
//!
//! `ConnectSession` reports every reach of a session this daemon facilitates through
//! [`SessionReach`]; an integration's own RPC resolves sessions and reaches the host's room services
//! through [`ReachableSessions`], implemented here once.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_extensions::{ReachableSessions, ReachedSession, SessionReach};
use tddy_daemon_livekit::session_room::{session_type_is_facilitated_here, SessionTerminalBridge};
use tddy_rpc::{RequestTransport, Status};

use crate::connection_service::DaemonSessionHost;

impl DaemonSessionHost {
    /// Install what extends a session's reach beyond this host (builder). Without it a session is
    /// reachable here and nowhere else.
    #[must_use]
    pub fn with_session_reach(mut self, reach: Arc<dyn SessionReach>) -> Self {
        self.debug_assert_rpc_families_not_installed("with_session_reach");
        self.session_reach = reach;
        self
    }
}

#[async_trait::async_trait]
impl ReachableSessions for DaemonSessionHost {
    async fn resolve(
        &self,
        session_token: &str,
        session_id: &str,
    ) -> Result<ReachedSession, Status> {
        let (session_dir, metadata) = self
            .launch_sessions()
            .resolve_session_for_token(session_token, session_id)?;
        // The rule `ConnectSession` applies before it reaches a session: a room for one whose agent
        // runs elsewhere would sit on a daemon that serves nobody.
        let session_type = metadata.session_type.as_deref().unwrap_or_default();
        if !session_type_is_facilitated_here(session_type) {
            return Err(Status::failed_precondition(format!(
                "session {session_id} is a {session_type:?} session, which this daemon does not facilitate, so it hosts no room for it"
            )));
        }
        let worktree_root = metadata.repo_path.map(PathBuf::from).ok_or_else(|| {
            Status::failed_precondition(format!(
                "session {session_id} records no checkout on this daemon"
            ))
        })?;
        Ok(ReachedSession {
            session_id: session_id.to_string(),
            session_dir,
            worktree_root,
            // The extension's own RPC received the request, not the core.
            arrived_over: RequestTransport::Direct,
        })
    }

    fn session_services(&self) -> Result<tddy_rpc::MultiRpcService, Status> {
        Arc::new(self.clone()).session_room_roster()
    }

    async fn expose_terminal(&self, session_id: &str) -> Result<(), Status> {
        SessionTerminalBridge::bridge(self, session_id).await
    }
}
