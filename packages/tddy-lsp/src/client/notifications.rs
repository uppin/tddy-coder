//! How a server's notifications reach the consumers that need them.
//!
//! Two consumers with incompatible needs read the same stream. The bridged restructuring path
//! *drains* them and folds `$/progress` and `experimental/serverStatus` into its own account of
//! what the server is doing; a status reader wants to watch the same phases without taking them
//! out of that account. A single destructive queue can serve one of them, never both — whichever
//! got there first steals from the other.
//!
//! So one sink serves both: a bounded backlog for the drainer, and a broadcast for any number of
//! observers.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde_json::Value;
use tokio::sync::broadcast;

/// How many undrained notifications to keep before dropping the oldest.
///
/// A consumer that never drains must not grow this without bound, and one that drains on a poll
/// only ever needs the recent few — rust-analyzer emits `$/progress` continuously while it loads.
/// The same bound governs how far a subscriber may fall behind before it starts losing them.
pub(crate) const NOTIFICATION_BACKLOG: usize = 256;

/// rust-analyzer's account of its own state: health, and whether it is quiescent.
const SERVER_STATUS: &str = "experimental/serverStatus";

/// What a subscriber is handed by [`NotificationStream::recv`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationEvent {
    /// One notification, verbatim as the server sent it.
    Received(Value),
    /// The subscriber fell behind: this many notifications were overwritten before it read them.
    ///
    /// Reported rather than skipped over. A subscriber slow enough to lose progress is exactly the
    /// one whose wait is about to end in a timeout, and a timeout that cannot say it lost sight of
    /// the server is the silence this account exists to replace.
    Lost(u64),
    /// The client is gone, so no further notification will arrive on this stream.
    Ended,
}

/// A non-destructive view of one server's notifications.
///
/// Reading is independent of every other subscriber and of
/// [`drain_notifications`](super::LspClient::drain_notifications): no read takes a notification
/// away from anyone else.
///
/// Attaching yields the notifications that arrive from then on, never the backlog already there —
/// a subscriber handed that would read a finished load as live news about a server that has since
/// moved on.
///
/// A subscriber that reads slower than the server reports does not hold the reader loop up. Once
/// [`NOTIFICATION_BACKLOG`] unread notifications have accumulated the oldest are overwritten, and
/// the next read reports [`NotificationEvent::Lost`] with how many went — so a caller that then
/// times out can say it lost sight of the server rather than merely failing to mention it.
pub struct NotificationStream {
    receiver: broadcast::Receiver<Value>,
}

impl NotificationStream {
    /// The next notification, or an account of why one is not being handed over.
    pub async fn recv(&mut self) -> NotificationEvent {
        match self.receiver.recv().await {
            Ok(notification) => NotificationEvent::Received(notification),
            Err(broadcast::error::RecvError::Lagged(lost)) => NotificationEvent::Lost(lost),
            Err(broadcast::error::RecvError::Closed) => NotificationEvent::Ended,
        }
    }
}

/// Where a notification the client does not consume itself is put, on its way to both kinds of
/// consumer: the backlog a drainer empties, and the broadcast subscribers watch.
pub(crate) struct NotificationSink {
    /// Kept for a single draining consumer, newest last, capped at [`NOTIFICATION_BACKLOG`].
    kept: Mutex<VecDeque<Value>>,
    /// Handed to every subscriber. Sending never blocks on a slow one, so no observer can stall
    /// the reader loop that feeds this.
    live: broadcast::Sender<Value>,
    /// The last `experimental/serverStatus` the server sent, whoever drained it.
    ///
    /// A status is sent on a transition and only then, and it is the one notification whose
    /// *latest* value is a fact about the server now rather than news: its health and whether it is
    /// quiescent. Both queues lose it — the drainer takes it from everyone after, and a long load's
    /// progress pushes it out of the backlog — so a second consumer of a warm server would never
    /// learn that its build scripts failed. Kept here, it can be read by anyone at any time.
    status: Mutex<Option<Value>>,
}

impl NotificationSink {
    pub(crate) fn new() -> Self {
        let (live, _) = broadcast::channel(NOTIFICATION_BACKLOG);
        Self {
            kept: Mutex::new(VecDeque::new()),
            live,
            status: Mutex::new(None),
        }
    }

    /// Record `notification` for both kinds of consumer.
    pub(crate) fn record(&self, notification: Value) {
        if notification.get("method").and_then(Value::as_str) == Some(SERVER_STATUS) {
            *self.status.lock().unwrap() = Some(notification.clone());
        }
        let mut kept = self.kept.lock().unwrap();
        if kept.len() == NOTIFICATION_BACKLOG {
            kept.pop_front();
        }
        kept.push_back(notification.clone());
        drop(kept);
        // Fails only when nobody is subscribed, which is the ordinary case for a drain-only
        // consumer and says nothing about the notification.
        let _ = self.live.send(notification);
    }

    /// Take every notification kept since the last drain, oldest first.
    pub(crate) fn drain(&self) -> Vec<Value> {
        self.kept.lock().unwrap().drain(..).collect()
    }

    /// The last `experimental/serverStatus` the server sent, if it has sent one.
    pub(crate) fn latest_status(&self) -> Option<Value> {
        self.status.lock().unwrap().clone()
    }

    /// Watch the notifications that arrive from now on, taking none of them from anyone.
    pub(crate) fn subscribe(&self) -> NotificationStream {
        NotificationStream {
            receiver: self.live.subscribe(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A subscriber that loses progress silently is how a wait ends in a timeout that cannot say
    /// where the server got to — the very failure keeping progress at all was meant to fix.
    #[tokio::test]
    async fn tells_a_subscriber_that_fell_behind_how_many_notifications_it_lost() {
        // Given a subscriber that has read nothing while the backlog filled and overflowed by one
        let sink = NotificationSink::new();
        let mut subscriber = sink.subscribe();
        for n in 0..=NOTIFICATION_BACKLOG {
            sink.record(json!({ "n": n }));
        }

        // When it reads
        let event = subscriber.recv().await;

        // Then it is told what it lost, rather than handed the remainder as if nothing had gone
        assert_eq!(event, NotificationEvent::Lost(1));
    }

    fn a_server_status(health: &str, quiescent: bool) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "experimental/serverStatus",
            "params": { "health": health, "quiescent": quiescent }
        })
    }

    #[test]
    fn keeps_the_latest_server_status_after_the_backlog_is_drained() {
        // Given a server that reported loading, then settled with a warning, and a drainer that
        // has already taken both
        let sink = NotificationSink::new();
        sink.record(a_server_status("ok", false));
        sink.record(a_server_status("warning", true));
        sink.drain();

        // When the status is asked for
        let status = sink.latest_status();

        // Then the last one the server sent is still there to be read
        assert_eq!(status, Some(a_server_status("warning", true)));
    }

    #[test]
    fn keeps_a_server_status_that_progress_pushed_out_of_the_backlog() {
        // Given a status followed by more progress than the backlog holds
        let sink = NotificationSink::new();
        sink.record(a_server_status("warning", true));
        for n in 0..=NOTIFICATION_BACKLOG {
            sink.record(json!({ "method": "$/progress", "params": { "n": n } }));
        }

        // When the status is asked for
        let status = sink.latest_status();

        // Then it survived the backlog overflowing
        assert_eq!(status, Some(a_server_status("warning", true)));
    }

    #[test]
    fn has_no_server_status_before_the_server_sends_one() {
        // Given a sink that has seen only progress
        let sink = NotificationSink::new();
        sink.record(json!({ "method": "$/progress", "params": {} }));

        // When the status is asked for
        let status = sink.latest_status();

        // Then there is none
        assert_eq!(status, None);
    }

    #[tokio::test]
    async fn tells_a_subscriber_when_the_client_it_watches_is_gone() {
        // Given a subscriber to a sink that is then dropped
        let sink = NotificationSink::new();
        let mut subscriber = sink.subscribe();
        drop(sink);

        // When it reads
        let event = subscriber.recv().await;

        // Then it is told the stream has ended rather than waiting forever
        assert_eq!(event, NotificationEvent::Ended);
    }
}
