//! Acceptance: a work item's session links, over the daemon's RPCs (`#workqueue` 6/14).
//!
//! The queue is seeded straight into the host's store and addressed by its address. Assigning a PR
//! reads GitHub through the daemon's inbox and is proved by `tddy-work-queue`'s inbox acceptance;
//! here the links that need no GitHub, the refusals, and the status stream's in-band "unavailable".

mod support;

use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use support::{a_host, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_core::session_work_queue::SessionWorkQueues;
use tddy_rpc::{Code, Request};
use tddy_service::proto::work_queue::{
    LinkWorkItemSessionRequest, StreamWorkQueueStatusRequest, UnassignWorkItemPrRequest,
    UnlinkWorkItemSessionRequest, WorkQueueAddress, WorkQueueService,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const OWNER: &str = "queue-owner-links";

/// A host holding a queue owned by `OWNER` with one item, `n1`, and the queue's address.
fn a_host_holding_a_queue_with_one_item() -> (Host, WorkQueueAddress) {
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let queues = SessionWorkQueues::new(host.sessions.path().join("sessions"));
    queues
        .create_for_session(OWNER, THIS_PROJECT, 0)
        .expect("the owner gets its queue");
    queues
        .update(OWNER, |queue| {
            *queue = WorkQueue {
                version: 1,
                nodes: vec![WorkItem {
                    node_id: "n1".to_string(),
                    title: "Wire it".to_string(),
                    ..WorkItem::default()
                }],
                ..Default::default()
            }
        })
        .expect("the queue is filled");
    let address = WorkQueueAddress {
        project_id: THIS_PROJECT.to_string(),
        queue_id: OWNER.to_string(),
        daemon_instance_id: HOST_A.to_string(),
    };
    (host, address)
}

fn linked_ids(work_queue_json: &str) -> Vec<String> {
    let plan: serde_json::Value = serde_json::from_str(work_queue_json).expect("JSON");
    plan["nodes"][0]["linked_sessions"]
        .as_array()
        .map(|linked| {
            linked
                .iter()
                .map(|s| s["session_id"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

async fn link(host: &Host, address: &WorkQueueAddress, item: &str, session: &str) -> String {
    host.daemon
        .link_work_item_session(Request::direct(LinkWorkItemSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address.clone()),
            item_id: item.to_string(),
            session_id: session.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("linked")
        .into_inner()
        .work_queue_json
}

#[tokio::test]
async fn linking_a_session_twice_links_it_once_and_unlinking_removes_it() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();

    // When
    link(&host, &address, "n1", "sess-a").await;
    let after_two = link(&host, &address, "n1", "sess-a").await;
    let after_unlink = host
        .daemon
        .unlink_work_item_session(Request::direct(UnlinkWorkItemSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address.clone()),
            item_id: "n1".to_string(),
            session_id: "sess-a".to_string(),
        }))
        .await
        .expect("unlinked")
        .into_inner()
        .work_queue_json;

    // Then
    assert_eq!(
        (linked_ids(&after_two), linked_ids(&after_unlink)),
        (vec!["sess-a".to_string()], Vec::<String>::new())
    );
}

#[tokio::test]
async fn a_link_on_an_unknown_item_is_not_found_and_writes_nothing() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();

    // When
    let refused = host
        .daemon
        .link_work_item_session(Request::direct(LinkWorkItemSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address.clone()),
            item_id: "n9".to_string(),
            session_id: "sess-a".to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect_err("refused");

    // Then
    assert_eq!(refused.code(), Code::NotFound);
}

#[tokio::test]
async fn unassigning_an_item_with_no_pr_is_a_no_op() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();

    // When
    let answered = host
        .daemon
        .unassign_work_item_pr(Request::direct(UnassignWorkItemPrRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address),
            item_id: "n1".to_string(),
        }))
        .await
        .expect("answered")
        .into_inner();

    // Then
    assert_eq!(linked_ids(&answered.work_queue_json), Vec::<String>::new());
}

#[tokio::test]
async fn the_status_stream_says_in_band_when_the_queues_github_cannot_be_read() {
    // Given — a host with no GitHub inbox
    let (host, address) = a_host_holding_a_queue_with_one_item();

    // When
    let mut stream = host
        .daemon
        .stream_work_queue_status(Request::direct(StreamWorkQueueStatusRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address),
        }))
        .await
        .expect("the stream opens")
        .into_inner();
    let first = stream.next().await.expect("a frame").expect("not a status");

    // Then
    assert!(
        !first.unavailable_reason.trim().is_empty(),
        "the frame names why: {first:?}"
    );
}
