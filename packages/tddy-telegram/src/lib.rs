//! Telegram as a session front-end: starting and controlling sessions from a chat, notifying on
//! agent activity, and the elicitation loop that asks an operator a question and waits.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 2 — 10 modules and 6,835 production lines, the
//! second-largest subsystem in that crate.
//!
//! Two facts made it an early node:
//!
//! - **It has no `connection.ConnectionService` surface at all.** Its RPCs are
//!   `tddy/v1/observer.proto` and `tddy/v1/presenter_intent.proto`, owned by `tddy-service`. So this
//!   move changes no wire coordinate and migrates no client.
//! - **It reached the 23,099-line god module for exactly one symbol** — `now_unix_ms`, at
//!   `telegram_session_subscriber.rs:122`. That symbol is now
//!   [`tddy_daemon_kernel::now_unix_ms`], published by node 1, which is the whole reason this
//!   subsystem can leave.
//!
//! `teloxide` is attributable to this subsystem alone and leaves `tddy-daemon` with it.
//!
//! # Direction of dependency
//!
//! `session_list_enrichment` and `session_notifications` reach the elicitation modules that move
//! here, and both stay behind in this node. That direction is correct — the session modules consume
//! `tddy-telegram` rather than the reverse — so no cycle is created.

use std::sync::Arc;

/// Anything that can deliver a message to an operator.
///
/// A trait rather than a concrete bot because the daemon injects a double in tests and a real
/// `teloxide` bot in production, and because the desktop host sends no lifecycle messages at all.
pub trait TelegramSender: Send + Sync {
    /// Deliver `text` to the configured chat.
    fn send(&self, text: &str) -> Result<(), TelegramError>;
}

/// Why a message could not be delivered.
#[derive(Debug, thiserror::Error)]
pub enum TelegramError {
    #[error("telegram is not configured")]
    NotConfigured,
    #[error("telegram rejected the message: {reason}")]
    Rejected { reason: String },
}

/// The hooks a daemon hands its session machinery so a chat can drive it.
#[derive(Clone)]
pub struct TelegramDaemonHooks {
    // TODO(model-telegram-screen): implement
    _sender: Option<Arc<dyn TelegramSender>>,
}

impl TelegramDaemonHooks {
    /// Build the hooks over a sender, or none when telegram is unconfigured.
    pub fn new(_sender: Option<Arc<dyn TelegramSender>>) -> Self {
        // TODO(model-telegram-screen): implement
        unimplemented!("TelegramDaemonHooks::new")
    }
}

/// Send the daemon's "started" / "stopped" lifecycle message.
///
/// Called from the daemon's HTTP server today. `docs/dev/todo/2026-09-05-…` records that the desktop
/// host cannot send it because window creation must not block on a Telegram HTTP call, and that
/// sharing it needs the message moved out of the server — which this extraction makes possible but
/// does not itself do.
pub async fn send_daemon_lifecycle_message(
    _sender: &dyn TelegramSender,
    _event: LifecycleEvent,
) -> Result<(), TelegramError> {
    // TODO(model-telegram-screen): implement
    unimplemented!("send_daemon_lifecycle_message")
}

/// Which lifecycle transition a message announces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    Started,
    Stopped,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A sender that records what it was asked to deliver, so a test can assert on the message
    /// rather than on the fact that a call happened.
    #[derive(Default)]
    struct ARecordingSender {
        sent: Mutex<Vec<String>>,
    }

    impl TelegramSender for ARecordingSender {
        fn send(&self, text: &str) -> Result<(), TelegramError> {
            self.sent.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn announces_that_the_daemon_started() {
        // Given
        let sender = ARecordingSender::default();

        // When
        send_daemon_lifecycle_message(&sender, LifecycleEvent::Started)
            .await
            .expect("a configured sender delivers");

        // Then
        let sent = sender.sent.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        assert!(
            sent[0].to_lowercase().contains("started"),
            "the message says what happened: {:?}",
            sent[0]
        );
    }

    /// An unconfigured telegram is not an error the daemon should fail to start over, but it is also
    /// not a silent success — the hooks are absent, and every call site checks.
    #[test]
    fn builds_hooks_that_carry_no_sender_when_telegram_is_unconfigured() {
        // When
        let hooks = TelegramDaemonHooks::new(None);

        // Then
        assert!(
            hooks._sender.is_none(),
            "unconfigured telegram carries no sender"
        );
    }
}
