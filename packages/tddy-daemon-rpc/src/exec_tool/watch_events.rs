//! The events an orchestrator's session watches matched, attached to every tool answer it receives
//! on this host as `watchEvents` (`#activity-wake` 2/3).
//!
//! Only a successful answer whose result is a JSON object carries them, and only for a caller
//! that is a session of this host holding a live watch; any other answer consumes nothing. A call a
//! peer forwarded here for a session it holds is that peer's to decorate, and finds no session — so
//! no watches — here.

use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};

use super::ExecToolRpcHandler;

/// The key the events ride under in an answer's result object.
pub(super) const WATCH_EVENTS_KEY: &str = "watchEvents";

impl ExecToolRpcHandler {
    /// `response` to `req`, with the caller's matched watch events attached when it has any.
    pub(super) async fn with_watch_events(
        &self,
        req: &ExecuteToolRequest,
        response: ExecuteToolResponse,
    ) -> ExecuteToolResponse {
        let Some(_watches) = self.project_home_host.session_watches.as_deref() else {
            return response;
        };
        // TODO(session-watches): implement — take the caller's delivery and insert it under
        // WATCH_EVENTS_KEY.
        let _ = (req, WATCH_EVENTS_KEY);
        response
    }
}
