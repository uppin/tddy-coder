//! `ReportOperatorNotification`: an agent notifying its operator in its own words.
//!
//! The caller is `tddy-tools notify`, run by the agent inside a daemon-launched claude-cli or
//! cursor-cli session with its credentials taken from the environment the launch exported. It is
//! authenticated exactly as `ReportSessionStatus` is — the same session-type check, then the same
//! `hook_token` comparison — so it grants nothing the session's hooks could not already do.
//!
//! Unlike the hook reports it refuses loudly: an over-long title or body is `INVALID_ARGUMENT` with
//! the reason, never truncated, because the CLI exits non-zero with it and the agent can try again.

use crate::service::ActivityServiceImpl;
use tddy_rpc::{Response, Status};
use tddy_service::proto::activity::{
    ReportOperatorNotificationRequest, ReportOperatorNotificationResponse,
};

impl ActivityServiceImpl {
    /// Validate, authenticate, then publish one `AttentionRequired` notification with source
    /// `Agent`, carrying the agent's title, body, level, sound flag and the session's web route.
    pub(super) async fn report_operator_notification_impl(
        &self,
        req: ReportOperatorNotificationRequest,
    ) -> Result<Response<ReportOperatorNotificationResponse>, Status> {
        // TODO(agent-notify): implement
        let _ = req;
        Err(Status::unimplemented(
            "TODO(agent-notify): ReportOperatorNotification is not implemented",
        ))
    }
}
