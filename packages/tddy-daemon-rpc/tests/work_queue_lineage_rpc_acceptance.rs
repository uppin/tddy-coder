//! Acceptance: a work queue's dependency edges and lineage, over the daemon's RPCs (`#workqueue` 9/14).
//!
//! The queue is seeded straight into the host's store and addressed by its address. Line edges read
//! from PR bases are proved by `tddy-work-queue`'s inbox acceptance; here the stored dependency edges,
//! their refusals, and the graph read.

mod support;

use pretty_assertions::assert_eq;
use support::{a_host, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_core::session_work_queue::SessionWorkQueues;
use tddy_rpc::{Code, Request};
use tddy_service::proto::work_queue::{
    GetWorkQueueGraphRequest, SetWorkQueueDependenciesRequest, WorkQueueAddress, WorkQueueService,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const OWNER: &str = "queue-owner-lineage";

fn an_item(id: &str, parents: &[&str]) -> WorkItem {
    WorkItem {
        node_id: id.to_string(),
        title: format!("Item {id}"),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..WorkItem::default()
    }
}

/// A host holding a queue owned by `OWNER`: n1, then n2 planned on n1; and the queue's address.
fn a_host_holding_a_two_item_queue() -> (Host, WorkQueueAddress) {
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let queues = SessionWorkQueues::new(host.sessions.path().join("sessions"));
    queues
        .create_for_session(OWNER, THIS_PROJECT, 0)
        .expect("the owner gets its queue");
    queues
        .update(OWNER, |queue| {
            *queue = WorkQueue {
                version: 1,
                nodes: vec![an_item("n1", &[]), an_item("n2", &["n1"])],
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

async fn set_needs(
    host: &Host,
    address: &WorkQueueAddress,
    item: &str,
    needs: &[&str],
) -> Result<String, tddy_rpc::Status> {
    host.daemon
        .set_work_queue_dependencies(Request::direct(SetWorkQueueDependenciesRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address.clone()),
            item_id: item.to_string(),
            needs: needs.iter().map(|n| n.to_string()).collect(),
        }))
        .await
        .map(|response| response.into_inner().work_queue_json)
}

fn needs_of(work_queue_json: &str, item: usize) -> Vec<String> {
    let plan: serde_json::Value = serde_json::from_str(work_queue_json).expect("JSON");
    plan["nodes"][item]["needs"]
        .as_array()
        .map(|needs| {
            needs
                .iter()
                .map(|n| n.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn an_items_needs_are_stored_on_the_queue_and_answered() {
    // Given
    let (host, address) = a_host_holding_a_two_item_queue();

    // When
    let after = set_needs(&host, &address, "n2", &["n1"])
        .await
        .expect("set");

    // Then
    assert_eq!(
        (needs_of(&after, 0), needs_of(&after, 1)),
        (Vec::<String>::new(), vec!["n1".to_string()])
    );
}

#[tokio::test]
async fn a_dependency_closing_a_cycle_is_refused_naming_it_and_writes_nothing() {
    // Given — n2 already needs n1
    let (host, address) = a_host_holding_a_two_item_queue();
    set_needs(&host, &address, "n2", &["n1"])
        .await
        .expect("set");

    // When — n1 would need n2
    let refused = set_needs(&host, &address, "n1", &["n2"])
        .await
        .expect_err("a cycle is refused");
    let unchanged = set_needs(&host, &address, "n2", &["n1"])
        .await
        .expect("set again");

    // Then
    assert_eq!(
        (
            refused.code(),
            refused.message().contains("n1 -> n2 -> n1"),
            needs_of(&unchanged, 0)
        ),
        (Code::FailedPrecondition, true, Vec::<String>::new())
    );
}

#[tokio::test]
async fn a_dependency_on_an_unknown_item_is_not_found() {
    // Given
    let (host, address) = a_host_holding_a_two_item_queue();

    // When
    let refused = set_needs(&host, &address, "n2", &["n9"])
        .await
        .expect_err("refused");

    // Then
    assert_eq!(refused.code(), Code::NotFound);
}

#[tokio::test]
async fn the_graph_of_an_unknown_queue_is_not_found() {
    // Given
    let (host, mut address) = a_host_holding_a_two_item_queue();
    address.queue_id = "no-such-queue".to_string();

    // When
    let refused = host
        .daemon
        .get_work_queue_graph(Request::direct(GetWorkQueueGraphRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address),
        }))
        .await
        .expect_err("refused");

    // Then
    assert_eq!(refused.code(), Code::NotFound);
}
