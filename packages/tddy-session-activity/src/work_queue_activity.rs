//! Moving a work item tells the queue's owner (`#workqueue` 8/14). A move — a reorder or a change of
//! parents, by the user or by an agent — is published as an activity of the queue's **owning session**,
//! of the session-own source `WorkQueue` (watch kind `work_queue`). The owner's agent can wait for it;
//! a move its own agent made is recorded but does not wake it.
//!
//! One kind with a change enum: later queue events (land, propagate, a schedule firing — `#workqueue`
//! 14/14) add [`WorkQueueChange`] variants, not watch kinds.

use std::sync::Arc;

use crate::session_notifications::SessionNotificationBus;

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
    pub fn publish_move(&self, event: &WorkQueueEvent) {
        // TODO(n8 move-activities): implement — a `SessionNotificationSource::WorkQueue` notification
        // (added with every registry the trunk-commit precedent lists) whose text names the item, the
        // change and the actor.
        let _ = (&self.bus, event);
        todo!("TODO(n8): implement WorkQueueActivities::publish_move")
    }
}

/// Whether `event` wakes its queue's owner: every move except one its own agent made.
pub fn wakes_owner(event: &WorkQueueEvent) -> bool {
    // TODO(n8 move-activities): implement
    let _ = event;
    todo!("TODO(n8): implement wakes_owner")
}
