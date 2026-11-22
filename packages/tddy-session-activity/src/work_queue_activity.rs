//! Moving a work item tells the queue's owner (`#workqueue` 8/14). A move — a reorder or a change of
//! parents, by the user or by an agent — is published as an activity of the queue's **owning session**,
//! of the session-own source `WorkQueue` (watch kind `work_queue`). The owner's agent can wait for it;
//! a move its own agent made is recorded but does not wake it.
//!
//! One kind with a change enum: later queue events (land, propagate, a schedule firing — `#workqueue`
//! 14/14) add [`WorkQueueChange`] variants, not watch kinds.

use std::sync::Arc;

use tddy_daemon_kernel::session_notification::{
    SessionNotificationKind, SessionNotificationSource,
};

use crate::session_notifications::{SessionNotification, SessionNotificationBus};

/// What changed about one item's position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkQueueChange {
    /// Moved in the reading order, 0-based positions.
    Reordered { from: u32, to: u32 },
    /// Its parents (planned bases) changed.
    ParentsChanged {
        before: Vec<String>,
        after: Vec<String>,
    },
}

/// Who moved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// The operator, in the browser.
    User { login: String },
    /// A session's agent, through a tool.
    Agent { session_id: String },
}

/// One move of one item of a session's queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkQueueEvent {
    pub queue_session_id: String,
    pub project_id: String,
    pub os_user: String,
    pub item_id: String,
    pub item_title: String,
    pub change: WorkQueueChange,
    pub actor: Actor,
}

/// Publishes queue moves as activities of the owning session.
pub struct WorkQueueActivities {
    bus: Arc<SessionNotificationBus>,
}

impl WorkQueueActivities {
    pub fn new(bus: Arc<SessionNotificationBus>) -> Self {
        Self { bus }
    }

    /// Publish `event` as an activity of `event.queue_session_id`.
    ///
    /// Its kind carries [`wakes_owner`]: `AttentionRequired` for a move anyone but the owner's own
    /// agent made, `Activity` — recorded, never delivered — for one its own agent made.
    pub async fn publish_move(&self, event: &WorkQueueEvent) {
        let kind = if wakes_owner(event) {
            SessionNotificationKind::AttentionRequired
        } else {
            SessionNotificationKind::Activity
        };
        let notification = SessionNotification {
            session_id: event.queue_session_id.clone(),
            os_user: event.os_user.clone(),
            label: event.queue_session_id.clone(),
            kind,
            source: SessionNotificationSource::WorkQueue,
            text: describe(event),
            agent: None,
            question: None,
            ended: None,
            activity_status: None,
            message: None,
            at_unix_ms: tddy_daemon_kernel::now_unix_ms(),
        };
        self.bus.publish(notification).await;
    }
}

/// Whether `event` wakes its queue's owner: every move except one its own agent made.
pub fn wakes_owner(event: &WorkQueueEvent) -> bool {
    !matches!(&event.actor, Actor::Agent { session_id } if session_id == &event.queue_session_id)
}

/// The one line a move is announced as, positions 1-based as the queue's view shows them.
fn describe(event: &WorkQueueEvent) -> String {
    let actor = match &event.actor {
        Actor::User { login } => login.clone(),
        Actor::Agent { session_id } => format!("session {session_id}"),
    };
    let change = match &event.change {
        WorkQueueChange::Reordered { from, to } => {
            format!("moved from position {} to {}", from + 1, to + 1)
        }
        WorkQueueChange::ParentsChanged { before, after } => format!(
            "parents changed from [{}] to [{}]",
            before.join(", "),
            after.join(", ")
        ),
    };
    format!(
        "Work item {} ({}) {change}, by {actor}",
        event.item_title, event.item_id
    )
}
