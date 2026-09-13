//! The subscriber every daemon has: the stream `tddy-web` reads.
//!
//! PRD: `docs/ft/daemon/session-notifications.md` (FR2, FR3, FR7).
//!
//! `tddy-daemon`'s Telegram subscriber is the other one, and it stayed there: it delivers through
//! the daemon's bot hooks and reads the daemon's `telegram:` config block, neither of which this
//! crate has or should acquire. It implements the same
//! [`crate::session_notifications::SessionNotificationSubscriber`] trait from the far side of the
//! crate boundary, which is the whole point of the trait.

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::session_notifications::{SessionNotification, SessionNotificationSubscriber};

/// How many notifications a client that has stopped reading may fall behind before its oldest are
/// dropped. Sized for a burst of tool-call activity across a drawer of sessions while a tab is
/// backgrounded; a client that overruns it loses the oldest events, which for an indicator is the
/// right loss — the newest event is the one the dot is derived from.
const NOTIFICATION_STREAM_CAPACITY: usize = 256;

/// The subscriber behind `StreamSessionNotifications`.
///
/// One broadcast channel carries every session on the daemon, so a drawer of any size pays for one
/// subscription (PRD NFR1), and every connected client gets its own copy of each event.
pub struct SessionNotificationStreamSubscriber {
    tx: broadcast::Sender<SessionNotification>,
}

impl Default for SessionNotificationStreamSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionNotificationStreamSubscriber {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(NOTIFICATION_STREAM_CAPACITY);
        Self { tx }
    }

    /// A receiver for one client's stream. Events published before this call are not replayed:
    /// notifications describe moments, and a dot derived from a replayed one would claim an agent
    /// is working now because it was working when the tab was last open.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionNotification> {
        self.tx.subscribe()
    }
}

#[async_trait]
impl SessionNotificationSubscriber for SessionNotificationStreamSubscriber {
    fn name(&self) -> &'static str {
        "notification-stream"
    }

    fn wants(&self, _notification: &SessionNotification) -> bool {
        true
    }

    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        // A send with nobody subscribed is the ordinary state of a daemon with no browser
        // attached, not a delivery failure.
        if self.tx.send(notification.clone()).is_err() {
            log::debug!(
                target: "tddy_daemon::session_notifications",
                "no notification stream client attached for session {}",
                notification.session_id
            );
        }
        Ok(())
    }

    fn client_relay(&self) -> Option<broadcast::Receiver<SessionNotification>> {
        Some(self.subscribe())
    }
}
