//! The session host as a [`SessionStarter`]: a satellite's operator start reaches the host's one
//! generic start (`LaunchSessions::start_session_for_os_user`) as a `StartSessionRequest` that asks
//! to be rejected on a branch conflict, so the satellite can ask its operator how to continue
//! (`#agent-tools 18/19`).
//!
//! The daemon binds one of these into its `LateBoundSessionStarter` once the host is built; a
//! satellite built before the host holds the late-bound port, never the host.

use std::sync::Arc;

use tddy_daemon_extensions::{OperatorSessionStart, OperatorStartOutcome, SessionStarter};
use tddy_rpc::Status;
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};

use super::DaemonSessionHost;

/// The host's generic start, offered to a satellite.
pub struct HostSessionStarter {
    host: Arc<DaemonSessionHost>,
}

impl HostSessionStarter {
    pub fn new(host: Arc<DaemonSessionHost>) -> Self {
        Self { host }
    }
}

#[async_trait::async_trait]
impl SessionStarter for HostSessionStarter {
    async fn start_for_operator(
        &self,
        start: OperatorSessionStart,
    ) -> Result<OperatorStartOutcome, Status> {
        // TODO(telegram-spec-launch): implement — `operator_start_request`, then
        // `self.host.launch_sessions().start_session_for_os_user(&start.os_user, …)` with a
        // discarding progress sink, then `operator_start_outcome`.
        let _ = (&self.host, start);
        todo!("telegram-spec-launch: HostSessionStarter::start_for_operator")
    }
}

/// The `StartSessionRequest` `start` stands for: its project, session type, model, first prompt and
/// branch intent, `on_branch_conflict = "reject"`, and no session token — an operator start is keyed
/// by its OS user.
pub fn operator_start_request(start: &OperatorSessionStart) -> StartSessionRequest {
    // TODO(telegram-spec-launch): implement
    let _ = start;
    todo!("telegram-spec-launch: operator_start_request")
}

/// What the generic start's answer means to the operator: the started session, or the branch
/// conflict it reported instead of creating one.
pub fn operator_start_outcome(response: StartSessionResponse) -> OperatorStartOutcome {
    // TODO(telegram-spec-launch): implement
    let _ = response;
    todo!("telegram-spec-launch: operator_start_outcome")
}
