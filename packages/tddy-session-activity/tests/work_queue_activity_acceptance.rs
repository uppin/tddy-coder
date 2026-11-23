//! Moving a work item tells the queue's owner (`#workqueue` 8/14). The source and kind are reached by
//! name (`"WorkQueue"`, `"work_queue"`) so this file compiles before `/green` adds them.

use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_session_activity::session_notifications::{
    RecordingSessionNotificationSubscriber, SessionNotificationBus,
};
use tddy_session_activity::session_watches::WatchKind;
use tddy_session_activity::work_queue_activity::{
    wakes_owner, Actor, WorkQueueActivities, WorkQueueChange, WorkQueueEvent,
};

const OWNER: &str = "sess-owner";

fn a_move(change: WorkQueueChange, actor: Actor) -> WorkQueueEvent {
    WorkQueueEvent {
        queue_session_id: OWNER.to_string(),
        project_id: "proj-1".to_string(),
        os_user: "dev".to_string(),
        item_id: "n3".to_string(),
        item_title: "Wire the inbox".to_string(),
        change,
        actor,
    }
}

fn the_user() -> Actor {
    Actor::User {
        login: "octocat".to_string(),
    }
}

/// The work-queue activities published, as (session, source, text).
async fn published_by(event: &WorkQueueEvent) -> Vec<(String, String, String)> {
    let recorded = Arc::new(RecordingSessionNotificationSubscriber::new());
    let bus = Arc::new(SessionNotificationBus::new().with_subscriber(Arc::clone(&recorded)));
    WorkQueueActivities::new(bus).publish(event).await;
    recorded
        .received()
        .into_iter()
        .map(|n| (n.session_id, format!("{:?}", n.source), n.text))
        .collect()
}

#[tokio::test]
async fn a_user_reorder_becomes_an_activity_of_the_queue_owner() {
    // When
    let published = published_by(&a_move(
        WorkQueueChange::Reordered { from: 2, to: 0 },
        the_user(),
    ))
    .await;

    // Then
    assert_eq!(published.len(), 1, "one activity: {published:?}");
    let (session, source, text) = &published[0];
    assert_eq!((session.as_str(), source.as_str()), (OWNER, "WorkQueue"));
    for word in ["Wire the inbox", "3", "1", "octocat"] {
        assert!(text.contains(word), "{text:?} names {word:?}");
    }
}

#[tokio::test]
async fn a_parents_change_names_the_parents_before_and_after() {
    // When
    let published = published_by(&a_move(
        WorkQueueChange::ParentsChanged {
            before: vec!["n1".to_string()],
            after: vec!["n1".to_string(), "n2".to_string()],
        },
        Actor::Agent {
            session_id: OWNER.to_string(),
        },
    ))
    .await;

    // Then
    let text = &published[0].2;
    assert!(text.contains("n1") && text.contains("n1, n2"), "{text:?}");
}

#[test]
fn the_owners_own_moves_do_not_wake_it_and_anyone_elses_do() {
    assert_eq!(
        (
            wakes_owner(&a_move(
                WorkQueueChange::Reordered { from: 0, to: 1 },
                Actor::Agent {
                    session_id: OWNER.to_string()
                }
            )),
            wakes_owner(&a_move(
                WorkQueueChange::Reordered { from: 0, to: 1 },
                the_user()
            )),
        ),
        (false, true)
    );
}

#[test]
fn the_work_queue_watch_kind_is_named_work_queue() {
    // When
    let kind = WatchKind::parse("work_queue");

    // Then
    assert_eq!(kind.map(|k| k.as_str()), Some("work_queue"));
}

/// `#workqueue` 14/14: landings, propagations, refusals and schedule firings are the same kind.
#[tokio::test]
async fn land_propagate_refusal_and_firing_reach_the_owner_naming_what_happened() {
    // Given one change of each kind, by a schedule someone armed
    let by_a_schedule = || Actor::Schedule {
        armed_by: "octocat".to_string(),
    };
    let changes = [
        (
            WorkQueueChange::Landed {
                sha: "abc1234".to_string(),
            },
            "abc1234",
        ),
        (
            WorkQueueChange::Propagated {
                onto: "master".to_string(),
            },
            "master",
        ),
        (
            WorkQueueChange::Refused {
                action: "propagate".to_string(),
                reason: "the local branch is dirty".to_string(),
            },
            "the local branch is dirty",
        ),
        (
            WorkQueueChange::ScheduleFired {
                action: "merge".to_string(),
            },
            "merge",
        ),
    ];

    for (change, named) in changes {
        // When
        let event = a_move(change, by_a_schedule());
        let published = published_by(&event).await;

        // Then each is one activity of the owner, naming the item, what happened and who armed it,
        // and it wakes the owner
        assert_eq!(published.len(), 1, "{published:?}");
        let (session, source, text) = &published[0];
        assert_eq!((session.as_str(), source.as_str()), (OWNER, "WorkQueue"));
        for word in ["Wire the inbox", named, "octocat"] {
            assert!(text.contains(word), "{text:?} names {word:?}");
        }
        assert!(wakes_owner(&event), "{text:?} wakes the owner");
    }
}
