//! The split topic's methods that something outside it still calls on the session host.
//!
//! The methods themselves live on [`SplitSessions`](super::split_ports::SplitSessions). Each one
//! here forwards to it, so a test that holds the host keeps its path, and the topic has one body.

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// [`SplitSessions::split_context_from_codebase_host`](super::split_ports::SplitSessions::split_context_from_codebase_host).
    #[cfg(test)]
    pub(crate) async fn split_context_from_codebase_host(
        &self,
        session_token: &str,
        codebase_session: &str,
        codebase_daemon: &str,
        agent: &str,
        verb: &str,
    ) -> Result<crate::context_sync::PrefetchedContext, tddy_rpc::Status> {
        self.split_sessions()
            .split_context_from_codebase_host(
                session_token,
                codebase_session,
                codebase_daemon,
                agent,
                verb,
            )
            .await
    }
}
