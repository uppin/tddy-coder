//! Acceptance: a work item's labels and keyed badges, over the daemon's RPCs (`#workqueue` 7/14).
//!
//! The queue is seeded straight into the host's store and addressed by its address, as the link
//! calls' suite does.

mod support;

use pretty_assertions::assert_eq;
use support::{a_host, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_core::session_work_queue::SessionWorkQueues;
use tddy_rpc::{Code, Request};
use tddy_service::proto::work_queue::{
    ClearWorkItemBadgeRequest, SetWorkItemBadgeRequest, SetWorkItemLabelsRequest, WorkQueueAddress,
    WorkQueueService,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const OWNER: &str = "queue-owner-annotations";

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
                ..WorkQueue::default()
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

async fn set_labels(
    host: &Host,
    address: &WorkQueueAddress,
    item: &str,
    labels: &[&str],
) -> Result<String, tddy_rpc::Status> {
    host.daemon
        .set_work_item_labels(Request::direct(SetWorkItemLabelsRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address.clone()),
            item_id: item.to_string(),
            labels: labels.iter().map(|l| l.to_string()).collect(),
        }))
        .await
        .map(|answer| answer.into_inner().work_queue_json)
}

async fn set_badge(
    host: &Host,
    address: &WorkQueueAddress,
    key: &str,
    text: &str,
    tone: &str,
) -> Result<String, tddy_rpc::Status> {
    host.daemon
        .set_work_item_badge(Request::direct(SetWorkItemBadgeRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address.clone()),
            item_id: "n1".to_string(),
            key: key.to_string(),
            text: text.to_string(),
            tone: tone.to_string(),
        }))
        .await
        .map(|answer| answer.into_inner().work_queue_json)
}

fn item_json(work_queue_json: &str) -> serde_json::Value {
    let plan: serde_json::Value = serde_json::from_str(work_queue_json).expect("JSON");
    plan["nodes"][0].clone()
}

#[tokio::test]
async fn setting_labels_answers_the_queue_with_them_trimmed_and_deduplicated() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();

    // When
    let answered = set_labels(&host, &address, "n1", &["api", " api ", "", "ui"])
        .await
        .expect("labelled");

    // Then
    assert_eq!(
        item_json(&answered)["labels"],
        serde_json::json!(["api", "ui"])
    );
}

#[tokio::test]
async fn setting_a_badge_twice_with_one_key_keeps_the_latest_and_clearing_removes_it() {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();

    // When
    set_badge(&host, &address, "qa", "pending", "info")
        .await
        .expect("set");
    let after_two = set_badge(&host, &address, "qa", "passed", "success")
        .await
        .expect("set again");
    let after_clear = host
        .daemon
        .clear_work_item_badge(Request::direct(ClearWorkItemBadgeRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address.clone()),
            item_id: "n1".to_string(),
            key: "qa".to_string(),
        }))
        .await
        .expect("cleared")
        .into_inner()
        .work_queue_json;

    // Then
    assert_eq!(
        (
            item_json(&after_two)["badges"].clone(),
            item_json(&after_clear).get("badges").cloned()
        ),
        (
            serde_json::json!([{ "key": "qa", "text": "passed", "tone": "success" }]),
            None
        )
    );
}

#[tokio::test]
async fn an_unknown_item_is_not_found_a_bad_tone_or_an_over_limit_set_is_invalid_and_nothing_is_written(
) {
    // Given
    let (host, address) = a_host_holding_a_queue_with_one_item();
    let too_many: Vec<String> = (0..17).map(|n| format!("label-{n}")).collect();
    let too_many: Vec<&str> = too_many.iter().map(String::as_str).collect();

    // When
    let unknown = set_labels(&host, &address, "n9", &["api"])
        .await
        .expect_err("refused");
    let bad_tone = set_badge(&host, &address, "qa", "x", "loud")
        .await
        .expect_err("refused");
    let over_limit = set_labels(&host, &address, "n1", &too_many)
        .await
        .expect_err("refused");
    let unchanged = set_labels(&host, &address, "n1", &["kept"])
        .await
        .expect("labelled");

    // Then
    assert_eq!(
        (
            unknown.code(),
            bad_tone.code(),
            over_limit.code(),
            item_json(&unchanged)["labels"].clone()
        ),
        (
            Code::NotFound,
            Code::InvalidArgument,
            Code::InvalidArgument,
            serde_json::json!(["kept"])
        )
    );
}
