//! Changes of a work item, published as activities of the queue's owner (`#workqueue` 8/14, 14/14).
//!
//! A move is a reorder or a change of parents, by the operator (the queue view's RPC) or by an
//! agent (the stack git tool). A landing, a propagation, a refusal of either, and a schedule firing
//! are published through an [`ActivitySink`] the land and propagate engines' daemon side carries. Each
//! is published after it happened, to the session that owns the queue;
//! [`tddy_session_activity::work_queue_activity`] decides what that does to its watch.

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
pub(crate) async fn publish_reorder(
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
    )
    .await;
}

/// Publish the parents of `item_id` changing from `before` to `after`, when they changed.
pub(crate) async fn publish_parents_changed(
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
    )
    .await;
}

/// Where the land and propagate engines' outcomes go, and who they are by (`#workqueue` 14/14).
#[derive(Clone)]
pub(crate) struct ActivitySink {
    pub(crate) bus: Arc<SessionNotificationBus>,
    pub(crate) actor: Actor,
}

impl ActivitySink {
    /// A sink on `bus` for `actor`, or none without a bus (a host that publishes nothing).
    pub(crate) fn on(bus: Option<&Arc<SessionNotificationBus>>, actor: Actor) -> Option<Self> {
        bus.map(|bus| Self {
            bus: Arc::clone(bus),
            actor,
        })
    }

    /// Publish `change` of `item_id`, titled from `stack`.
    pub(crate) async fn publish(
        &self,
        queue: &MovedQueue<'_>,
        item_id: &str,
        stack: &WorkQueue,
        change: WorkQueueChange,
    ) {
        publish(
            Some(&self.bus),
            queue,
            item_id,
            stack,
            change,
            self.actor.clone(),
        )
        .await;
    }
}

async fn publish(
    bus: Option<&Arc<SessionNotificationBus>>,
    queue: &MovedQueue<'_>,
    item_id: &str,
    stack: &WorkQueue,
    change: WorkQueueChange,
    actor: Actor,
) {
    let Some(bus) = bus else { return };
    WorkQueueActivities::new(Arc::clone(bus))
        .publish(&WorkQueueEvent {
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
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::changeset::WorkItem;
    use tddy_session_activity::session_notifications::{
        RecordingSessionNotificationSubscriber, SessionNotificationKind,
    };

    const OWNER: &str = "sess-owner";

    fn a_queue_of(order: &[&str]) -> WorkQueue {
        WorkQueue {
            version: 1,
            nodes: order
                .iter()
                .enumerate()
                .map(|(index, id)| WorkItem {
                    node_id: (*id).to_string(),
                    title: format!("Title of {id}"),
                    display_order: Some(index as u32),
                    ..WorkItem::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    async fn published<F>(run: F) -> Vec<(SessionNotificationKind, String, String)>
    where
        F: AsyncFnOnce(Option<&Arc<SessionNotificationBus>>, &MovedQueue<'_>),
    {
        let recorded = Arc::new(RecordingSessionNotificationSubscriber::new());
        let bus = Arc::new(SessionNotificationBus::new().with_subscriber(Arc::clone(&recorded)));
        run(
            Some(&bus),
            &MovedQueue {
                queue_session_id: OWNER,
                project_id: "proj-1",
                os_user: "dev",
            },
        )
        .await;
        recorded
            .received()
            .into_iter()
            .map(|n| (n.kind, n.session_id, n.text))
            .collect()
    }

    #[tokio::test]
    async fn a_reorder_by_the_user_wakes_the_queues_owner() {
        let moved = published(async |bus, queue| {
            publish_reorder(
                bus,
                queue,
                "n3",
                &a_queue_of(&["n1", "n2", "n3"]),
                &a_queue_of(&["n1", "n3", "n2"]),
                Actor::User {
                    login: "octocat".to_string(),
                },
            )
            .await
        })
        .await;

        assert_eq!(moved.len(), 1, "{moved:?}");
        let (kind, session, text) = &moved[0];
        assert_eq!(
            (*kind, session.as_str()),
            (SessionNotificationKind::AttentionRequired, OWNER)
        );
        assert!(
            text.contains("Title of n3") && text.contains("octocat"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn a_move_that_changed_nothing_publishes_nothing() {
        let moved = published(async |bus, queue| {
            let same = a_queue_of(&["n1", "n2"]);
            publish_reorder(
                bus,
                queue,
                "n1",
                &same,
                &same,
                Actor::User { login: "x".into() },
            )
            .await;
            publish_parents_changed(
                bus,
                queue,
                "n2",
                (vec!["n1".to_string()], vec!["n1".to_string()]),
                &same,
                Actor::Agent {
                    session_id: "other".to_string(),
                },
            )
            .await;
        })
        .await;

        assert_eq!(moved, vec![]);
    }

    #[tokio::test]
    async fn the_owners_own_agent_changing_parents_is_recorded_without_waking_it() {
        let moved = published(async |bus, queue| {
            publish_parents_changed(
                bus,
                queue,
                "n2",
                (vec![], vec!["n1".to_string()]),
                &a_queue_of(&["n1", "n2"]),
                Actor::Agent {
                    session_id: OWNER.to_string(),
                },
            )
            .await
        })
        .await;

        assert_eq!(moved.len(), 1, "{moved:?}");
        assert_eq!(moved[0].0, SessionNotificationKind::Activity);
    }
}
