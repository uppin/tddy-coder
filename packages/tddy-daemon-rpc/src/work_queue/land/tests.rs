use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use tddy_changeset::work_queue_fixtures::{a_stack_host, A_PROJECT};
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_work_queue::schedule::{ConditionState, ScheduleView};
use tddy_work_queue::schedule_runner::ScheduleRunner;

use super::*;

/// The engines' port: records what would have been merged or propagated.
#[derive(Default)]
struct ARecordingEngine {
    ran: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl FireAction for ARecordingEngine {
    async fn run(&self, schedule: &Schedule) -> Result<(), String> {
        let what = match schedule.kind {
            ScheduleKind::Merge { .. } => "merge",
            ScheduleKind::Propagate => "propagate",
        };
        self.ran
            .lock()
            .unwrap()
            .push(format!("{what} {}", schedule.item_id));
        Ok(())
    }
}

fn an_armed(item: &str, kind: ScheduleKind) -> Schedule {
    Schedule {
        item_id: item.to_string(),
        kind,
        armed_at: "2026-11-23T10:00:00Z".to_string(),
        armed_by: "octocat".to_string(),
    }
}

fn a_view(schedule: &Schedule, status: ConditionState) -> ScheduleView {
    ScheduleView {
        schedule: schedule.clone(),
        conditions: Vec::new(),
        status,
        held_up_on: None,
    }
}

#[tokio::test]
async fn a_met_armed_merge_runs_the_engine_once_and_is_disarmed_in_the_store() {
    // Given a queue with n1 armed to merge and n2 armed to propagate
    let host = a_stack_host();
    let merge = an_armed("n1", ScheduleKind::Merge { skip_review: false });
    let propagate = an_armed("n2", ScheduleKind::Propagate);
    let queue = WorkQueue {
        version: 1,
        nodes: ["n1", "n2"]
            .map(|id| WorkItem {
                node_id: id.to_string(),
                ..WorkItem::default()
            })
            .to_vec(),
        schedules: vec![merge.clone(), propagate.clone()],
        ..WorkQueue::default()
    };
    host.orchestrator("owner", queue);
    let store = tddy_core::work_queue_store::WorkQueueStore::new(host.base());
    let engine = Arc::new(ARecordingEngine::default());
    let runner = ScheduleRunner::new(Arc::new(DaemonFireSchedule {
        store: store.clone(),
        address: tddy_service::proto::work_queue::WorkQueueAddress {
            project_id: A_PROJECT.to_string(),
            queue_id: "owner".to_string(),
            daemon_instance_id: String::new(),
        },
        os_user: "dev".to_string(),
        notification_bus: None,
        action: engine.clone(),
    }));

    // When n1's last condition turns met, the event is seen twice by two evaluations, and n2
    // is still waiting
    let first = runner
        .on_change(&[
            a_view(&merge, ConditionState::Met),
            a_view(&propagate, ConditionState::Waiting),
        ])
        .await;
    let again = runner
        .on_change(&[a_view(&merge, ConditionState::Met)])
        .await;

    // Then the engine merged n1 once, n1 is no longer armed in the store, n2 still is
    let armed: Vec<String> = store
        .read(A_PROJECT, "owner")
        .unwrap()
        .stack
        .schedules
        .into_iter()
        .map(|s| s.item_id)
        .collect();
    assert_eq!(
        (first, again, engine.ran.lock().unwrap().clone(), armed),
        (
            vec!["n1".to_string()],
            Vec::<String>::new(),
            vec!["merge n1".to_string()],
            vec!["n2".to_string()]
        )
    );
}

#[tokio::test]
async fn two_runners_seeing_the_same_arming_met_run_the_engine_once() {
    // Given one armed merge and two watchers (two runners over the same store)
    let host = a_stack_host();
    let merge = an_armed("n1", ScheduleKind::Merge { skip_review: false });
    host.orchestrator(
        "owner",
        WorkQueue {
            version: 1,
            nodes: vec![WorkItem {
                node_id: "n1".to_string(),
                ..WorkItem::default()
            }],
            schedules: vec![merge.clone()],
            ..WorkQueue::default()
        },
    );
    let store = tddy_core::work_queue_store::WorkQueueStore::new(host.base());
    let engine = Arc::new(ARecordingEngine::default());
    let runner = || {
        ScheduleRunner::new(Arc::new(DaemonFireSchedule {
            store: store.clone(),
            address: tddy_service::proto::work_queue::WorkQueueAddress {
                project_id: A_PROJECT.to_string(),
                queue_id: "owner".to_string(),
                daemon_instance_id: String::new(),
            },
            os_user: "dev".to_string(),
            notification_bus: None,
            action: engine.clone(),
        }))
    };

    // When both see it met
    runner()
        .on_change(&[a_view(&merge, ConditionState::Met)])
        .await;
    runner()
        .on_change(&[a_view(&merge, ConditionState::Met)])
        .await;

    // Then the engine ran once
    assert_eq!(
        engine.ran.lock().unwrap().clone(),
        vec!["merge n1".to_string()]
    );
}

#[tokio::test]
async fn a_firing_tells_the_queues_owner_once_naming_who_armed_it() {
    use tddy_session_activity::session_notifications::{
        RecordingSessionNotificationSubscriber, SessionNotificationKind,
    };
    // Given n1 armed to merge by octocat, and a bus the owner's activities are recorded on
    let host = a_stack_host();
    let merge = an_armed("n1", ScheduleKind::Merge { skip_review: false });
    host.orchestrator(
        "owner",
        WorkQueue {
            version: 1,
            nodes: vec![WorkItem {
                node_id: "n1".to_string(),
                title: "Wire the inbox".to_string(),
                ..WorkItem::default()
            }],
            schedules: vec![merge.clone()],
            ..WorkQueue::default()
        },
    );
    let recorded = Arc::new(RecordingSessionNotificationSubscriber::new());
    let bus = Arc::new(SessionNotificationBus::new().with_subscriber(Arc::clone(&recorded)));
    let runner = ScheduleRunner::new(Arc::new(DaemonFireSchedule {
        store: tddy_core::work_queue_store::WorkQueueStore::new(host.base()),
        address: tddy_service::proto::work_queue::WorkQueueAddress {
            project_id: A_PROJECT.to_string(),
            queue_id: "owner".to_string(),
            daemon_instance_id: String::new(),
        },
        os_user: "dev".to_string(),
        notification_bus: Some(bus),
        action: Arc::new(ARecordingEngine::default()),
    }));

    // When it is seen met twice
    for _ in 0..2 {
        runner
            .on_change(&[a_view(&merge, ConditionState::Met)])
            .await;
    }

    // Then the owner got one activity that wakes it, naming the item, the merge and who armed it
    let received = recorded.received();
    assert_eq!(received.len(), 1, "{received:?}");
    let fired = &received[0];
    assert_eq!(
        (fired.session_id.as_str(), fired.kind),
        ("owner", SessionNotificationKind::AttentionRequired)
    );
    for word in ["Wire the inbox", "merge", "octocat"] {
        assert!(fired.text.contains(word), "{:?} names {word:?}", fired.text);
    }
}
