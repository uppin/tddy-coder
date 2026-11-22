//! Acceptance: arming, listing and disarming merges and propagations over the daemon's RPCs
//! (`#workqueue` 13/14).
//!
//! The test host has no GitHub, so no schedule's facts can be read: each is shown `waiting` on
//! `facts`, with why — never met. The conditions themselves are proved over hand-built snapshots by
//! `tddy-work-queue`'s `schedule_unit`; here the wire: what is stored, what is refused, what the
//! status stream carries.

mod support;

use pretty_assertions::assert_eq;
use support::{a_host, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_core::session_work_queue::SessionWorkQueues;
use tddy_rpc::{Code, Request};
use tddy_service::proto::work_queue::{
    ListWorkQueueSchedulesRequest, ScheduleWorkItemRequest, ScheduleWorkQueueWaveRequest,
    UnscheduleWorkItemRequest, WorkQueueAddress, WorkQueueService,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const OWNER: &str = "queue-owner-schedules";

fn an_item(id: &str, needs: &[&str]) -> WorkItem {
    WorkItem {
        node_id: id.to_string(),
        title: format!("Item {id}"),
        branch: Some(format!("feat/{id}")),
        needs: needs.iter().map(|n| n.to_string()).collect(),
        ..WorkItem::default()
    }
}

struct AQueue {
    host: Host,
    address: WorkQueueAddress,
}

fn a_queue_of_two_items() -> AQueue {
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let sessions_base = host.sessions.path().join("sessions");
    let queues = SessionWorkQueues::new(sessions_base);
    queues
        .create_for_session(OWNER, THIS_PROJECT, 0)
        .expect("the owner gets its queue");
    queues
        .update(OWNER, |queue| {
            *queue = WorkQueue {
                version: 1,
                nodes: vec![an_item("n1", &[]), an_item("n2", &["n1"])],
                ..Default::default()
            }
        })
        .expect("the queue is filled");
    AQueue {
        host,
        address: WorkQueueAddress {
            project_id: THIS_PROJECT.to_string(),
            queue_id: OWNER.to_string(),
            daemon_instance_id: HOST_A.to_string(),
        },
    }
}

impl AQueue {
    fn arm(&self, item: &str, kind: &str, skip_review: bool) -> ScheduleWorkItemRequest {
        ScheduleWorkItemRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(self.address.clone()),
            item_id: item.to_string(),
            kind: kind.to_string(),
            skip_review,
        }
    }

    async fn listed(&self) -> Vec<(String, String, String)> {
        self.host
            .daemon
            .list_work_queue_schedules(Request::direct(ListWorkQueueSchedulesRequest {
                session_token: TEST_TOKEN.to_string(),
                queue: Some(self.address.clone()),
            }))
            .await
            .expect("list")
            .into_inner()
            .schedules
            .into_iter()
            .map(|s| (s.item_id, s.kind, s.status))
            .collect()
    }
}

#[tokio::test]
async fn an_armed_item_is_listed_and_waits_on_facts_it_cannot_read() {
    // Given
    let queue = a_queue_of_two_items();

    // When
    let armed = queue
        .host
        .daemon
        .schedule_work_item(Request::direct(queue.arm("n1", "merge", true)))
        .await
        .expect("arm")
        .into_inner()
        .schedule
        .expect("the schedule");

    // Then
    assert_eq!(
        (
            armed.kind.as_str(),
            armed.skip_review,
            armed.status.as_str(),
            armed.held_up_on.as_str(),
            armed.armed_by.is_empty(),
            queue.listed().await,
        ),
        (
            "merge",
            true,
            "waiting",
            "facts",
            false,
            vec![("n1".to_string(), "merge".to_string(), "waiting".to_string())]
        )
    );
}

#[tokio::test]
async fn arming_an_item_again_replaces_what_was_armed_and_unscheduling_disarms_it() {
    // Given
    let queue = a_queue_of_two_items();
    for kind in ["merge", "propagate"] {
        queue
            .host
            .daemon
            .schedule_work_item(Request::direct(queue.arm("n2", kind, false)))
            .await
            .expect("arm");
    }
    let armed_as = queue.listed().await;

    // When
    queue
        .host
        .daemon
        .unschedule_work_item(Request::direct(UnscheduleWorkItemRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(queue.address.clone()),
            item_id: "n2".to_string(),
        }))
        .await
        .expect("disarm");

    // Then
    assert_eq!(
        (
            armed_as.len(),
            armed_as[0].1.as_str(),
            queue.listed().await.len()
        ),
        (1, "propagate", 0)
    );
}

#[tokio::test]
async fn a_bad_request_is_refused_and_stores_nothing() {
    // Given
    let queue = a_queue_of_two_items();

    // When
    let unknown_item = queue
        .host
        .daemon
        .schedule_work_item(Request::direct(queue.arm("n9", "merge", false)))
        .await
        .unwrap_err();
    let unknown_kind = queue
        .host
        .daemon
        .schedule_work_item(Request::direct(queue.arm("n1", "land", false)))
        .await
        .unwrap_err();
    let skip_on_propagate = queue
        .host
        .daemon
        .schedule_work_item(Request::direct(queue.arm("n1", "propagate", true)))
        .await
        .unwrap_err();
    let disarm_unarmed = queue
        .host
        .daemon
        .unschedule_work_item(Request::direct(UnscheduleWorkItemRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(queue.address.clone()),
            item_id: "n1".to_string(),
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(
        (
            unknown_item.code(),
            unknown_kind.code(),
            skip_on_propagate.code(),
            disarm_unarmed.code(),
            queue.listed().await.len(),
        ),
        (
            Code::NotFound,
            Code::InvalidArgument,
            Code::InvalidArgument,
            Code::NotFound,
            0
        )
    );
}

#[tokio::test]
async fn arming_a_wave_without_a_readable_github_is_refused_rather_than_guessing_open_prs() {
    // Given
    let queue = a_queue_of_two_items();

    // When
    let status = queue
        .host
        .daemon
        .schedule_work_queue_wave(Request::direct(ScheduleWorkQueueWaveRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(queue.address.clone()),
            wave: 1,
            skip_review: false,
        }))
        .await
        .unwrap_err();
    let past_the_end = queue
        .host
        .daemon
        .schedule_work_queue_wave(Request::direct(ScheduleWorkQueueWaveRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(queue.address.clone()),
            wave: 9,
            skip_review: false,
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(
        (
            status.code(),
            past_the_end.code(),
            queue.listed().await.len()
        ),
        (Code::FailedPrecondition, Code::NotFound, 0)
    );
}
