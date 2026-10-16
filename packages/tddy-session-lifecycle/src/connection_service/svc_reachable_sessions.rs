//! The host as an extension sees it, and the builder that installs one.
//!
//! `ConnectSession` reports every reach of a session this daemon facilitates through
//! [`SessionReach`]; an integration's own RPC resolves sessions and reaches the host's room services
//! through [`ReachableSessions`], implemented here once.

use std::sync::Arc;

use tddy_daemon_extensions::{ReachableSessions, ReachedSession, SessionReach};
use tddy_rpc::Status;

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
        let _ = (session_token, session_id);
        todo!("DaemonSessionHost::resolve")
    }

    fn session_services(&self) -> Result<tddy_rpc::MultiRpcService, Status> {
        todo!("DaemonSessionHost::session_services")
    }

    async fn expose_terminal(&self, session_id: &str) -> Result<(), Status> {
        let _ = session_id;
        todo!("DaemonSessionHost::expose_terminal")
    }
}
