//! The message line a child on a target reports to its orchestrator with (`#orchestra` 2/9): the
//! followed hook-log line [`tddy_core::REMOTE_MESSAGE_EVENT`], posted as the session message
//! `PostSessionMessage` from the child would be.
//!
//! Kept apart from [`crate::remote_hook_follower`], whose split is reserved for a sweep.

use crate::back_channel::RemoteMessage;
use crate::remote_hook_follower::{HookLine, HookReportDelivery};

/// The report `line` carries, written by `session_id`'s agent as `os_user`: its payload's `text`.
/// `None` for a line whose payload has no non-empty text.
pub(crate) fn message_of(
    line: &HookLine,
    session_id: &str,
    os_user: &str,
) -> Option<RemoteMessage> {
    let text = line.payload.get("text")?.as_str()?;
    (!text.is_empty()).then(|| RemoteMessage {
        session_id: session_id.to_string(),
        os_user: os_user.to_string(),
        text: text.to_string(),
    })
}

/// Post the message `line` reports through `delivery`. A line with no text is dropped and a post
/// the delivery refuses is logged; neither stops the next hook arriving.
pub(crate) async fn post_reported_message(
    delivery: &dyn HookReportDelivery,
    line: &HookLine,
    session_id: &str,
    os_user: &str,
) {
    let Some(message) = message_of(line, session_id, os_user) else {
        log::warn!("session {session_id}: a message line without text was dropped");
        return;
    };
    if let Err(error) = delivery.post_message_to_orchestrator(message).await {
        log::warn!("session {session_id}: its report to its orchestrator was refused: {error}");
    }
}
