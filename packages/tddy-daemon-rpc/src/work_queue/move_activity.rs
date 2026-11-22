//! Moves of a work item, published as activities of the queue's owner (`#workqueue` 8/14).
//!
//! A move is a reorder or a change of parents, by the operator (the queue view's RPC) or by an
//! agent (the stack git tool). Each is published after it succeeded, to the session that owns the
//! queue; [`tddy_session_activity::work_queue_activity`] decides what that does to its watch.

use std::sync::Arc;

use tddy_core::changeset::WorkQueue;
use tddy_session_activity::session_notifications::SessionNotificationBus;
use tddy_session_activity::work_queue_activity::{
    Actor, WorkQueueActivities, WorkQueueChange, WorkQueueEvent,
};

/// Who a move is published for: the queue, and the project and OS user its owner belongs to.
pub(crate) struct MovedQueue<'a> {
    pub(crate) queue_session_id: &'a str,
    pub(crate) project_id: &'a str,
    pub(crate) os_user: &'a str,
}

/// Publish the reorder of `item_id` between `before` and `after`, when it changed position.
pub(crate) fn publish_reorder(
    bus: Option<&Arc<SessionNotificationBus>>,
    queue: &MovedQueue<'_>,
    item_id: &str,
    before: &WorkQueue,
    after: &WorkQueue,
    actor: Actor,
) {
    let position = |stack: &WorkQueue| stack.display_order().iter().position(|id| id == item_id);
    let (Some(from), Some(to)) = (position(before), position(after)) else {
        return;
    };
    if from == to {
        return;
    }
    publish(
        bus,
        queue,
        item_id,
        after,
        WorkQueueChange::Reordered {
            from: from as u32,
            to: to as u32,
        },
        actor,
    );
}

/// Publish the parents of `item_id` changing from `before` to `after`, when they changed.
pub(crate) fn publish_parents_changed(
    bus: Option<&Arc<SessionNotificationBus>>,
    queue: &MovedQueue<'_>,
    item_id: &str,
    parents: (Vec<String>, Vec<String>),
    stack: &WorkQueue,
    actor: Actor,
) {
    let (before, after) = parents;
    if before == after {
        return;
    }
    publish(
        bus,
        queue,
        item_id,
        stack,
        WorkQueueChange::ParentsChanged { before, after },
        actor,
    );
}

fn publish(
    bus: Option<&Arc<SessionNotificationBus>>,
    queue: &MovedQueue<'_>,
    item_id: &str,
    stack: &WorkQueue,
    change: WorkQueueChange,
    actor: Actor,
) {
    let Some(bus) = bus else { return };
    WorkQueueActivities::new(Arc::clone(bus)).publish_move(&WorkQueueEvent {
        queue_session_id: queue.queue_session_id.to_string(),
        project_id: queue.project_id.to_string(),
        os_user: queue.os_user.to_string(),
        item_id: item_id.to_string(),
        item_title: stack
            .node(item_id)
            .map(|node| node.title.clone())
            .unwrap_or_default(),
        change,
        actor,
    });
}
