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
use crate::session_notifications::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
};
use tddy_daemon_kernel::agent_notification_limits::validate_agent_notification;
use tddy_daemon_kernel::session_notification::{AgentNotification, NotificationLevel};
use tddy_rpc::{Response, Status};
use tddy_service::proto::activity::{
    NotificationLevel as ProtoNotificationLevel, ReportOperatorNotificationRequest,
    ReportOperatorNotificationResponse,
};

impl ActivityServiceImpl {
    /// Validate, authenticate, then publish one `AttentionRequired` notification with source
    /// `Agent`, carrying the agent's title, body, level, sound flag and the session's web route.
    pub(super) async fn report_operator_notification_impl(
        &self,
        req: ReportOperatorNotificationRequest,
    ) -> Result<Response<ReportOperatorNotificationResponse>, Status> {
        validate_agent_notification(&req.title, &req.body_markdown)
            .map_err(Status::invalid_argument)?;

        let (sessions_base, session_dir) = self.hook_session_dir(&req.os_user, &req.session_id)?;
        let meta = self.hook_session(&session_dir)?;

        // Refused before the credential, as `ReportSessionStatus` does: a notify run in a session
        // of the wrong type is told that rather than that its token is bad.
        let session_type = meta.session_type.as_deref().unwrap_or("");
        if session_type != "claude-cli" && session_type != "cursor-cli" {
            return Err(Status::failed_precondition(
                "session_type is not claude-cli or cursor-cli",
            ));
        }
        if !hook_token_matches(meta.hook_token.as_deref(), &req.hook_token) {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        log::debug!(
            target: "tddy_daemon::connection_service",
            "report_operator_notification: session={}",
            req.session_id
        );

        // Owned by `req.os_user`, the user whose sessions directory the token was just checked
        // against, so the notification stream relays it to that operator only.
        let link = format!(
            "#/sessions/{}?host={}",
            req.session_id, self.ports.daemon_instance_id
        );
        self.publish_for_session(&sessions_base, &req.session_id, |label| {
            Some(notification_for_agent(
                &req,
                label,
                link,
                tddy_daemon_kernel::now_unix_ms(),
            ))
        })
        .await;

        Ok(Response::new(ReportOperatorNotificationResponse {
            ok: true,
        }))
    }
}

/// The notification an agent's own `notify` raises: attention-worthy, in the agent's words, with
/// the title as the one-line `text` every surface that knows nothing of agent notifications shows.
fn notification_for_agent(
    req: &ReportOperatorNotificationRequest,
    label: &str,
    link: String,
    at_unix_ms: u64,
) -> SessionNotification {
    let title = req.title.trim().to_string();
    SessionNotification {
        session_id: req.session_id.clone(),
        os_user: req.os_user.clone(),
        label: label.to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::Agent,
        text: title.clone(),
        agent: Some(AgentNotification {
            title,
            body_markdown: req.body_markdown.clone(),
            level: level_from_wire(req.level),
            sound: req.sound,
            link,
        }),
        at_unix_ms,
    }
}

/// An unspecified or unknown level is the neutral one: the agent named none.
fn level_from_wire(level: i32) -> NotificationLevel {
    match ProtoNotificationLevel::try_from(level) {
        Ok(ProtoNotificationLevel::Success) => NotificationLevel::Success,
        Ok(ProtoNotificationLevel::Warning) => NotificationLevel::Warning,
        Ok(ProtoNotificationLevel::Error) => NotificationLevel::Error,
        Ok(ProtoNotificationLevel::Info | ProtoNotificationLevel::Unspecified) | Err(_) => {
            NotificationLevel::Info
        }
    }
}

/// Whether `presented` is the session's recorded hook token. A session that recorded none — or an
/// empty one — matches nothing, so an empty credential never passes for a session without hooks.
/// Compared in constant time over the bytes, so a refusal does not time how much of it matched.
fn hook_token_matches(recorded: Option<&str>, presented: &str) -> bool {
    let Some(recorded) = recorded.filter(|token| !token.is_empty()) else {
        return false;
    };
    recorded.len() == presented.len()
        && recorded
            .bytes()
            .zip(presented.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}
