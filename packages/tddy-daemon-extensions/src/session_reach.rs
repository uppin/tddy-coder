//! A session was reached on this host; an integration may extend that reach beyond it.
//!
//! The core reports every reach — a client connecting to a session — with the transport the request
//! arrived on, as the receiving host stamped it. What that costs is the integration's decision, not
//! the core's: LiveKit opens the session's room in the caller's time for a caller that arrived over
//! LiveKit, and in the background for anyone else, so a local client never waits on it.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_rpc::{RequestTransport, Status};

/// A session this host runs, at the moment something reached it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReachedSession {
    pub session_id: String,
    pub session_dir: PathBuf,
    /// The checkout the session's agent works in, on this host.
    pub worktree_root: PathBuf,
    /// How the request that reached it got here. Host-stamped: a caller cannot choose it.
    pub arrived_over: RequestTransport,
}

/// The session host, as an extension sees it: what an extension may ask of the host it extends.
///
/// Implemented once, by the session host, in wiring.
#[async_trait::async_trait]
pub trait ReachableSessions: Send + Sync {
    /// Authenticate `session_token` and resolve `session_id` to a session this host runs — for an
    /// extension serving an RPC of its own. `arrived_over` of the result is
    /// [`RequestTransport::Direct`]: the extension's RPC, not the core, received the request.
    async fn resolve(&self, session_token: &str, session_id: &str)
        -> Result<ReachedSession, Status>;

    /// The RPC surface a participant outside this host reaches the session through.
    fn session_services(&self) -> Result<tddy_rpc::MultiRpcService, Status>;

    /// Make the session's terminal drivable from outside this host. A session with no terminal is
    /// not a failure.
    async fn expose_terminal(&self, session_id: &str) -> Result<(), Status>;
}

/// Something that extends a session's reach beyond this host.
#[async_trait::async_trait]
pub trait SessionReach: Send + Sync {
    /// `session` was reached. An `Err` fails the request that reached it, so an implementation
    /// returns one only when that caller cannot be served without what failed.
    async fn session_reached(
        &self,
        session: ReachedSession,
        host: Arc<dyn ReachableSessions>,
    ) -> Result<(), Status>;
}

/// No extension wired: a session is reachable on this host and nowhere else.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSessionReach;

#[async_trait::async_trait]
impl SessionReach for NoSessionReach {
    async fn session_reached(
        &self,
        _session: ReachedSession,
        _host: Arc<dyn ReachableSessions>,
    ) -> Result<(), Status> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host an extension must not touch: every call is a failure of the test.
    struct AHostNothingMayAsk;

    #[async_trait::async_trait]
    impl ReachableSessions for AHostNothingMayAsk {
        async fn resolve(&self, _: &str, _: &str) -> Result<ReachedSession, Status> {
            panic!("NoSessionReach must not resolve sessions")
        }

        fn session_services(&self) -> Result<tddy_rpc::MultiRpcService, Status> {
            panic!("NoSessionReach must not ask for the session's services")
        }

        async fn expose_terminal(&self, _: &str) -> Result<(), Status> {
            panic!("NoSessionReach must not expose a terminal")
        }
    }

    fn a_session_reached_over(transport: RequestTransport) -> ReachedSession {
        ReachedSession {
            session_id: "1780828020298-abc".to_string(),
            session_dir: PathBuf::from("/sessions/1780828020298-abc"),
            worktree_root: PathBuf::from("/worktrees/my-app/feat-x"),
            arrived_over: transport,
        }
    }

    #[tokio::test]
    async fn with_nothing_wired_a_reach_from_anywhere_succeeds_without_asking_the_host() {
        // Given no extension, and a session reached by a remote client
        let reach = NoSessionReach;

        // When it is reported
        let reached = reach
            .session_reached(
                a_session_reached_over(RequestTransport::LiveKit),
                Arc::new(AHostNothingMayAsk),
            )
            .await;

        // Then it is answered as reached, and the host was never consulted
        reached.expect("a reach with nothing to extend it must succeed");
    }
}
