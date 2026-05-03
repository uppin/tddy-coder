//! Per-chat **telegram-tracked session** gate and structured traffic logging (PRD).
//!
//! **Lifecycle (tracked session):**
//! - **Set:** when the operator starts a workflow from Telegram (**`handle_start_workflow`**) for that chat, or completes **Enter session** (**`bind_chat_to_session_for_telegram_tracking`**).
//! - **Clear:** when that chat’s tracked session id matches and the workflow hits **WorkflowComplete** (success or failure),
//!   when **handle_delete_session** removes that session, or when `clear_telegram_tracked_session_for_chat` is invoked
//!   for an explicit leave / future control-plane hook.
//!
//! Shared coordinator wiring mirrors [`crate::active_elicitation::SharedActiveElicitationCoordinator`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

/// Shared coordinator between teloxide dispatch, session control harness, and notifier.
pub type SharedTelegramTrackedSessionCoordinator = Arc<StdMutex<TelegramTrackedSessionCoordinator>>;

/// One-line structured log for **inbound** Telegram user messages (no raw secrets).
pub fn format_inbound_message_traffic_log(
    chat_id: i64,
    text_len: usize,
    session_hint: Option<&str>,
) -> String {
    let sid = session_hint.unwrap_or("n/a");
    format!(
        "telegram_traffic direction=inbound kind=message chat_id={chat_id} session_id={sid} text_len={text_len}"
    )
}

/// Log [`log::Record`] target for inbound Telegram **message text** (route in daemon `log:` policies).
pub const TELEGRAM_INBOUND_MESSAGE_BODY_LOG_TARGET: &str =
    "tddy_daemon::telegram_bot::message-body";

/// One-line structured log for **inbound** Telegram message body (full text; route to a restricted sink).
pub fn format_inbound_telegram_message_body_log(chat_id: i64, text: &str) -> String {
    format!("telegram_inbound_message_body chat_id={chat_id} text={text}")
}

/// Emit full inbound message text on [`TELEGRAM_INBOUND_MESSAGE_BODY_LOG_TARGET`] for YAML logger routing.
pub fn log_inbound_telegram_message_body(chat_id: i64, text: &str) {
    log::info!(
        target: TELEGRAM_INBOUND_MESSAGE_BODY_LOG_TARGET,
        "{}",
        format_inbound_telegram_message_body_log(chat_id, text)
    );
}

/// One-line structured log for **inbound** Telegram callback queries.
pub fn format_inbound_callback_traffic_log(
    chat_id: i64,
    callback_len: usize,
    session_id: Option<&str>,
    callback_prefix: &str,
) -> String {
    let sid = session_id.unwrap_or("n/a");
    format!(
        "telegram_traffic direction=inbound kind=callback chat_id={chat_id} session_id={sid} callback_len={callback_len} callback_prefix={callback_prefix}"
    )
}

/// One-line structured log when the daemon receives a presenter **ModeChanged** for Telegram routing.
pub fn format_inbound_mode_changed_traffic_log(chat_id: i64, session_id: &str) -> String {
    format!(
        "telegram_traffic direction=inbound kind=mode_changed chat_id={chat_id} session_id={session_id}"
    )
}

/// In-memory per-chat association: Telegram `chat_id` → workflow `session_id` the operator chose
/// via **Enter session**.
#[derive(Debug, Default)]
pub struct TelegramTrackedSessionCoordinator {
    tracked: HashMap<i64, String>,
}

impl TelegramTrackedSessionCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Establish tracking after **Enter session** (same `session_id` string as metadata / callbacks).
    pub fn bind_chat_to_session_for_telegram_tracking(&mut self, chat_id: i64, session_id: &str) {
        let sid = session_id.trim().to_string();
        log::info!(
            target: "tddy_daemon::telegram",
            "telegram_tracked_session: bind chat_id={} session_id={}",
            chat_id,
            sid
        );
        self.tracked.insert(chat_id, sid);
    }

    pub fn tracked_session_for_chat(&self, chat_id: i64) -> Option<String> {
        self.tracked.get(&chat_id).cloned()
    }

    /// When `true`, outbound Telegram must omit workflow action inline keyboards and use **Enter session** UI instead.
    ///
    /// Policy: suppress when the chat has **no** tracked session, or the tracked session is **not** the outbound target
    /// (multi-session safety on a shared Telegram channel).
    pub fn should_suppress_workflow_keyboards_for_session(
        &self,
        chat_id: i64,
        target_session_id: &str,
    ) -> bool {
        let target = target_session_id.trim();
        match self.tracked.get(&chat_id) {
            None => {
                log::debug!(
                    target: "tddy_daemon::telegram",
                    "telegram_tracked_session: suppress keyboards (no tracked session) chat_id={} target_session_id={}",
                    chat_id,
                    target
                );
                true
            }
            Some(tracked) if tracked.trim() != target => {
                log::info!(
                    target: "tddy_daemon::telegram",
                    "telegram_tracked_session: suppress keyboards (wrong tracked session) chat_id={} tracked_session_id={} target_session_id={}",
                    chat_id,
                    tracked,
                    target
                );
                true
            }
            Some(tracked) => {
                log::debug!(
                    target: "tddy_daemon::telegram",
                    "telegram_tracked_session: allow workflow keyboards chat_id={} session_id={}",
                    chat_id,
                    tracked
                );
                false
            }
        }
    }

    /// Clear tracking for a chat (explicit leave / reset hooks).
    pub fn clear_telegram_tracked_session_for_chat(&mut self, chat_id: i64) {
        if self.tracked.remove(&chat_id).is_some() {
            log::info!(
                target: "tddy_daemon::telegram",
                "telegram_tracked_session: cleared tracked session chat_id={}",
                chat_id
            );
        } else {
            log::debug!(
                target: "tddy_daemon::telegram",
                "telegram_tracked_session: clear no-op (not tracked) chat_id={}",
                chat_id
            );
        }
    }

    /// Drop tracking on every chat that was bound to `session_id` (workflow finished / deleted).
    pub fn clear_all_chats_tracked_to_session(&mut self, session_id: &str) {
        let needle = session_id.trim().to_string();
        self.tracked.retain(|cid, sid| {
            if sid == &needle {
                log::info!(
                    target: "tddy_daemon::telegram",
                    "telegram_tracked_session: cleared tracked session (session lifecycle) chat_id={} session_id={}",
                    cid,
                    needle
                );
                false
            } else {
                true
            }
        });
    }

    /// Structured preview line for **outbound** Telegram traffic (tests + log capture under `tddy_daemon::telegram`).
    pub fn stub_structured_traffic_log_preview(&self, chat_id: i64, session_id: &str) -> String {
        format!(
            "telegram_traffic direction=outbound kind=mode_changed_preview chat_id={chat_id} session_id={}",
            session_id.trim()
        )
    }

    /// Invoked after **Enter session** succeeds; returns whether the harness/notifier should run elicitation replay.
    pub fn notify_enter_session_elicitation_replay_skeleton(
        &mut self,
        chat_id: i64,
        session_id: &str,
    ) -> bool {
        log::info!(
            target: "tddy_daemon::telegram",
            "telegram_tracked_session: enter session requests elicitation replay chat_id={} session_id={}",
            chat_id,
            session_id.trim()
        );
        true
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn bind_chat_to_session_persists_tracked_session_id() {
        let mut c = TelegramTrackedSessionCoordinator::new();
        let chat = 99_i64;
        let sid = "01900000-0000-7000-8000-000000000099";
        c.bind_chat_to_session_for_telegram_tracking(chat, sid);
        assert_eq!(
            c.tracked_session_for_chat(chat).as_deref(),
            Some(sid),
            "green: bind_chat_to_session_for_telegram_tracking must persist session_id for chat_id"
        );
    }

    #[test]
    fn suppress_workflow_keyboards_when_chat_has_no_tracked_session() {
        let c = TelegramTrackedSessionCoordinator::new();
        assert!(
            c.should_suppress_workflow_keyboards_for_session(
                42,
                "01900000-0000-7000-8000-000000000042"
            ),
            "green: untracked chat must suppress workflow keyboards for outbound session targeting"
        );
    }

    #[test]
    fn traffic_log_preview_includes_direction_labels() {
        let c = TelegramTrackedSessionCoordinator::new();
        let line = c.stub_structured_traffic_log_preview(7, "01900000-0000-7000-8000-000000000007");
        assert!(
            line.contains("direction=outbound"),
            "green: structured telegram traffic must label outbound events; got {line:?}"
        );
    }

    #[test]
    fn inbound_message_body_log_target_ends_with_message_body_suffix() {
        assert!(
            TELEGRAM_INBOUND_MESSAGE_BODY_LOG_TARGET.ends_with("message-body"),
            "green: YAML log policies match this suffix; got {:?}",
            TELEGRAM_INBOUND_MESSAGE_BODY_LOG_TARGET
        );
    }

    #[test]
    fn inbound_message_body_log_line_includes_chat_id_and_text() {
        let line = format_inbound_telegram_message_body_log(-99, "ping");
        assert!(
            line.contains("chat_id=-99") && line.contains("text=ping"),
            "green: message-body log must carry chat_id and full text; got {line:?}"
        );
    }

    #[test]
    fn enter_session_requests_elicitation_replay() {
        let mut c = TelegramTrackedSessionCoordinator::new();
        assert!(
            c.notify_enter_session_elicitation_replay_skeleton(
                1,
                "01900000-0000-7000-8000-000000000001"
            ),
            "green: Enter session must signal elicitation replay scheduling"
        );
    }
}
