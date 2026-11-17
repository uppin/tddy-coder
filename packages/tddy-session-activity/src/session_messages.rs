//! Messages between sessions, read as activity: each session's inbox is a position over the
//! project notification log (`#activity-wake` 5/5).
//!
//! A message is stored once, as a `message` record in the project log
//! ([`crate::session_notifications::SessionNotificationSource::Message`], carrying a
//! `SessionMessage`), its `session_id` the sender. Nothing else keeps it. A session receives it
//! two ways, both reading that record:
//!
//! - **Watching.** A session's **inbox watch** ([`WatchFilter::inbox_of`]: kind `message`, naming
//!   the session itself) matches the messages addressed to it, and is delivered like any watch — on
//!   tool answers, by the wake, or streamed by `tddy-tools monitor --kinds message`
//!   (`WaitSessionWatchEvents`). Any session may hold one; every other watch is an orchestrator's.
//!   An orchestrator also hears what its sessions send through an ordinary watch that includes the
//!   kind `message`.
//! - **Reading.** `ReadSessionMessages` reads through the session's **inbox position**
//!   ([`SessionWatchStore::read_messages`]): the `seq` up to which it has read the messages
//!   addressed to it. The position is a reference into the log, kept with the watches in
//!   `session-watches.json`; it holds no message.
//!
//! **Delivered once.** A message addressed to a session is handed to it once across its watches
//! and its reads: a watch skips a message to its owner that the inbox position has passed, and a
//! read skips one a watch of the reader has already delivered.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md

use crate::project_notification_log::{ProjectNotificationLog, ProjectNotificationRecord};
use crate::session_watches::{SessionWatchStore, WatchFilter, WatchKind, WatchOwner};
use tddy_daemon_kernel::session_notification::SessionMessage;

/// The longest a message's text may be, in bytes: 8 KiB.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;

/// How many messages one read hands over; the rest wait for the next.
pub const MAX_MESSAGES_PER_READ: usize = 50;

/// Refuse a message `text` that is empty or longer than [`MAX_MESSAGE_BYTES`], saying what to send
/// instead.
pub fn refuse_a_bad_message_text(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Err(
            "a message needs a text: give PostSessionMessage the message to send".to_string(),
        );
    }
    if text.len() > MAX_MESSAGE_BYTES {
        return Err(format!(
            "a message is at most 8 KiB ({MAX_MESSAGE_BYTES} bytes), not {} — send less, or split it",
            text.len()
        ));
    }
    Ok(())
}

/// The `message` part of `record`, when it is one addressed to `session_id`.
pub(crate) fn a_message_to<'a>(
    record: &'a ProjectNotificationRecord,
    session_id: &str,
) -> Option<&'a SessionMessage> {
    if record.source != crate::session_notifications::SessionNotificationSource::Message {
        return None;
    }
    record
        .message
        .as_ref()
        .filter(|message| message.to_session_id == session_id)
}

impl WatchFilter {
    /// The inbox watch of `session_id`: the messages addressed to it. The one watch a session that
    /// is not an orchestrator may hold.
    pub fn inbox_of(session_id: &str) -> Self {
        Self {
            session_ids: vec![session_id.to_string()],
            started_by_me: false,
            kinds: vec![WatchKind::Message],
        }
    }

    /// Whether this filter is [`Self::inbox_of`] `session_id`.
    pub fn is_inbox_of(&self, session_id: &str) -> bool {
        *self == Self::inbox_of(session_id)
    }
}

/// One message, as a read hands it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxMessage {
    /// The message's record in the project log.
    pub seq: u64,
    pub message_id: String,
    /// The sender.
    pub from_session_id: String,
    /// The sender's display label.
    pub from: String,
    /// The message this one replies to.
    pub reply_to: Option<String>,
    pub text: String,
    pub sent_at_unix_ms: u64,
}

impl InboxMessage {
    /// The message `record` carries, or `None` for a record that is not a message.
    pub fn of(record: &ProjectNotificationRecord) -> Option<Self> {
        let message = record.message.as_ref()?;
        Some(Self {
            seq: record.seq,
            message_id: message.message_id.clone(),
            from_session_id: record.session_id.clone(),
            from: record.label.clone(),
            reply_to: message.reply_to.clone(),
            text: message.text.clone(),
            sent_at_unix_ms: record.at_unix_ms,
        })
    }
}

/// What one `ReadSessionMessages` hands its reader, read as it is answered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InboxRead {
    /// The unread messages addressed to the reader, oldest first, each once; at most
    /// [`MAX_MESSAGES_PER_READ`].
    pub messages: Vec<InboxMessage>,
    /// Messages addressed to the reader that the log evicted before they could be read.
    pub lost: u64,
    /// Whether more unread messages wait after these.
    pub has_more: bool,
}

impl SessionWatchStore {
    /// `ReadSessionMessages` for `recipient`: the messages addressed to it after its inbox
    /// position — or, with `after_message_id`, after that message, read or not — oldest first, at
    /// most `limit` (default and cap [`MAX_MESSAGES_PER_READ`]), skipping any a watch of
    /// `recipient` already delivered. The position advances past what is answered, under the
    /// project's lock, and is persisted with the watches.
    ///
    /// Refused for an `after_message_id` `recipient` never received.
    pub async fn read_messages(
        &self,
        recipient: &WatchOwner,
        after_message_id: Option<&str>,
        limit: Option<usize>,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
    ) -> Result<InboxRead, String> {
        let limit = limit
            .unwrap_or(MAX_MESSAGES_PER_READ)
            .min(MAX_MESSAGES_PER_READ);
        let (start, skip_watch_delivered) = match after_message_id {
            Some(message_id) => {
                let named = self
                    .received_message(recipient, message_id, log)
                    .ok_or_else(|| {
                        format!(
                            "you received no message {message_id}: after_message_id must name a \
                             message addressed to you (ReadSessionMessages answers message_id, or \
                             your message watch carries one)"
                        )
                    })?;
                (named.seq, false)
            }
            None => (self.inbox_position(recipient).await, true),
        };
        Ok(self
            .read_inbox(
                recipient,
                start,
                limit,
                skip_watch_delivered,
                log,
                now_unix_ms,
            )
            .await)
    }

    /// The `seq` up to which `recipient` has read the messages addressed to it; `0` for a session
    /// that never read.
    pub async fn inbox_position(&self, recipient: &WatchOwner) -> u64 {
        let project = self.project_of(&recipient.project_id);
        let state = project.lock().await;
        crate::session_watches::inbox_position_of(&state, recipient)
    }

    /// The message `message_id` if `recipient` received it — one addressed to it that the log still
    /// holds — for the reply rule: a session may reply to the sender of a message it received.
    pub fn received_message(
        &self,
        recipient: &WatchOwner,
        message_id: &str,
        log: &ProjectNotificationLog,
    ) -> Option<InboxMessage> {
        let (records, _) = log.records_after(&recipient.project_id, &recipient.os_user, 0);
        records
            .iter()
            .filter(|record| {
                a_message_to(record, &recipient.session_id)
                    .is_some_and(|message| message.message_id == message_id)
            })
            .find_map(InboxMessage::of)
    }
}
