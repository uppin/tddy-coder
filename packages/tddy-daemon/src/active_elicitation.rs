//! Per-chat **active elicitation lease** / ordered queue (PRD: single visible question per Telegram chat).
//!
//! Outbound [`crate::telegram_notifier::TelegramSessionWatcher`] and inbound
//! [`crate::telegram_session_control::TelegramSessionControlHarness`] share one
//! [`ActiveElicitationCoordinator`] per process (see `main.rs` wiring).

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

/// Log a warning when a chat's elicitation queue grows past this (FIFO depth).
const ELICITATION_QUEUE_WARN_DEPTH: usize = 10;

/// Shared handle used by the notifier and Telegram session control harness.
pub type SharedActiveElicitationCoordinator = Arc<StdMutex<ActiveElicitationCoordinator>>;

/// Owns, per Telegram `chat_id`, an ordered queue of workflow `session_id` values waiting for
/// elicitation surface. The **first** entry is the session that may show the primary interactive
/// surface (full `eli:s:` inline keyboard where applicable).
///
/// The same `session_id` may appear **multiple times** when the presenter emits several elicitation
/// [`ModeChanged`](tddy_service::gen::ModeChanged) events in a row (multi-step clarification): each
/// outbound registration pushes one token; completing one step must not rotate the queue away from
/// that session until all its tokens are drained when elicitation ends (see
/// [`Self::drain_elicitation_completion_for_session`]).
#[derive(Debug, Default)]
pub struct ActiveElicitationCoordinator {
    /// FIFO per chat: front = active token holder.
    queues: HashMap<i64, Vec<String>>,
}

impl ActiveElicitationCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `session_id` needs one elicitation surface slot for `chat_id` (outbound path).
    ///
    /// Pushes even when `session_id` is already queued so multi-step clarification reserves one
    /// token per presenter prompt; duplicate Telegram payloads are still suppressed upstream via
    /// signature dedupe before registration.
    pub fn register_elicitation_surface_request(&mut self, chat_id: i64, session_id: String) {
        let q = self.queues.entry(chat_id).or_default();
        q.push(session_id.clone());
        let len = q.len();
        log::info!(
            target: "tddy_daemon::active_elicitation",
            "register_elicitation_surface_request: chat_id={} session_id={} queue_len={} active_session_id={:?}",
            chat_id,
            session_id,
            len,
            q.first().map(String::as_str)
        );
        if len > ELICITATION_QUEUE_WARN_DEPTH {
            log::warn!(
                target: "tddy_daemon::active_elicitation",
                "register_elicitation_surface_request: queue depth high chat_id={} queue_len={} (warn_threshold={})",
                chat_id,
                len,
                ELICITATION_QUEUE_WARN_DEPTH
            );
        }
    }

    /// Session id that owns the active elicitation token for `chat_id`, if any.
    pub fn active_session_for_chat(&self, chat_id: i64) -> Option<String> {
        self.queues.get(&chat_id).and_then(|q| q.first()).cloned()
    }

    /// Whether an elicitation callback for `session_id` may be honored for `chat_id`.
    pub fn elicitation_callback_permitted(&self, chat_id: i64, session_id: &str) -> bool {
        self.active_session_for_chat(chat_id)
            .as_deref()
            .map(|a| a == session_id)
            .unwrap_or(false)
    }

    /// Pop **one** queued surface token from the front when it matches `completed_session_id`, and
    /// return the new active session id (see [`Self::drain_elicitation_completion_for_session`] for
    /// clearing every token when presenter elicitation ends).
    pub fn advance_after_elicitation_completion(
        &mut self,
        chat_id: i64,
        completed_session_id: &str,
    ) -> Option<String> {
        let q = match self.queues.get_mut(&chat_id) {
            Some(q) if !q.is_empty() => q,
            _ => {
                log::debug!(
                    target: "tddy_daemon::active_elicitation",
                    "advance_after_elicitation_completion: empty or missing queue chat_id={}",
                    chat_id
                );
                return None;
            }
        };
        if q[0] != completed_session_id {
            log::info!(
                target: "tddy_daemon::active_elicitation",
                "advance_after_elicitation_completion: head mismatch chat_id={} queue_head={} completed={} — not rotating",
                chat_id,
                q[0],
                completed_session_id
            );
            return None;
        }
        q.remove(0);
        let next = q.first().cloned();
        match &next {
            Some(sid) => {
                log::info!(
                    target: "tddy_daemon::active_elicitation",
                    "advance_after_elicitation_completion: chat_id={} new_active_session_id={}",
                    chat_id,
                    sid
                );
            }
            None => {
                log::info!(
                    target: "tddy_daemon::active_elicitation",
                    "advance_after_elicitation_completion: chat_id={} queue drained",
                    chat_id
                );
                self.queues.remove(&chat_id);
            }
        }
        next
    }

    /// After `completed_session_id` **fully** leaves presenter elicitation, remove every consecutive
    /// queued surface token for that session from the front (see [`Self::register_elicitation_surface_request`]).
    ///
    /// Used when [`crate::telegram_notifier::TelegramSessionWatcher`] observes `was_eliciting &&
    /// !now_eliciting` so all tokens for a multi-step clarification clear in one transition.
    pub fn drain_elicitation_completion_for_session(
        &mut self,
        chat_id: i64,
        completed_session_id: &str,
    ) -> Option<String> {
        while self.active_session_for_chat(chat_id).as_deref() == Some(completed_session_id) {
            self.advance_after_elicitation_completion(chat_id, completed_session_id);
        }
        self.active_session_for_chat(chat_id)
    }
}

/// Whether outbound Telegram may attach the **primary** `eli:s:` / `eli:o:` inline keyboard for this
/// session in this chat (queued sessions get a deferred text-only notice instead).
pub fn should_emit_primary_elicitation_keyboard(
    coordinator: &ActiveElicitationCoordinator,
    chat_id: i64,
    session_id: &str,
) -> bool {
    coordinator.active_session_for_chat(chat_id).as_deref() == Some(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_registered_session_becomes_active_for_chat() {
        let mut c = ActiveElicitationCoordinator::new();
        c.register_elicitation_surface_request(
            424242,
            "01900000-0000-7000-8000-0000000000aa".into(),
        );
        assert_eq!(
            c.active_session_for_chat(424242).as_deref(),
            Some("01900000-0000-7000-8000-0000000000aa"),
            "first elicitation request for a chat should own the active token until completion"
        );
    }

    #[test]
    fn callback_permitted_only_for_active_session_when_second_is_queued() {
        let mut c = ActiveElicitationCoordinator::new();
        let sid_a = "01900000-0000-7000-8000-0000000000aa";
        let sid_b = "01900000-0000-7000-8000-0000000000bb";
        c.register_elicitation_surface_request(424242, sid_a.into());
        c.register_elicitation_surface_request(424242, sid_b.into());
        assert!(
            c.elicitation_callback_permitted(424242, sid_a),
            "active session callbacks must be permitted"
        );
        assert!(
            !c.elicitation_callback_permitted(424242, sid_b),
            "queued session must not receive elicitation callback until promoted"
        );
    }

    #[test]
    fn advance_after_completion_promotes_next_queued_session() {
        let mut c = ActiveElicitationCoordinator::new();
        let sid_a = "01900000-0000-7000-8000-0000000000aa";
        let sid_b = "01900000-0000-7000-8000-0000000000bb";
        c.register_elicitation_surface_request(424242, sid_a.into());
        c.register_elicitation_surface_request(424242, sid_b.into());
        assert_eq!(
            c.advance_after_elicitation_completion(424242, sid_a)
                .as_deref(),
            Some(sid_b)
        );
    }

    #[test]
    fn primary_keyboard_suppressed_for_queued_session() {
        let mut c = ActiveElicitationCoordinator::new();
        let sid_a = "01900000-0000-7000-8000-0000000000aa";
        let sid_b = "01900000-0000-7000-8000-0000000000bb";
        c.register_elicitation_surface_request(424242, sid_a.into());
        c.register_elicitation_surface_request(424242, sid_b.into());
        assert!(
            should_emit_primary_elicitation_keyboard(&c, 424242, sid_a),
            "active session should emit full primary keyboard"
        );
        assert!(
            !should_emit_primary_elicitation_keyboard(&c, 424242, sid_b),
            "queued session must not emit a competing primary eli:s keyboard"
        );
    }

    /// When the same workflow sends a second clarification [`ModeChanged`], outbound Telegram calls
    /// `register_elicitation_surface_request` again — pushing a **second** queue token for that
    /// session (signature dedupe still suppresses identical repeats upstream). If an inbound path
    /// incorrectly ran `advance_after_elicitation_completion` after only the first answer, the queue
    /// must not be fully drained — `should_emit_primary_elicitation_keyboard` must stay true for the
    /// next prompt of that session (operators otherwise see **elicitation queued** with no buttons).
    #[test]
    fn primary_keyboard_survives_duplicate_register_then_advance_same_session_multi_question() {
        let mut c = ActiveElicitationCoordinator::new();
        let chat = 424242_i64;
        let sid = "01900000-0000-7000-8000-0000000000aa";
        c.register_elicitation_surface_request(chat, sid.into());
        c.register_elicitation_surface_request(chat, sid.into());
        c.advance_after_elicitation_completion(chat, sid);
        assert!(
            should_emit_primary_elicitation_keyboard(&c, chat, sid),
            "same-session multi-step clarification must keep the primary Telegram surface until the session truly finishes elicitation"
        );
    }
}
