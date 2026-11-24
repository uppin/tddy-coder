//! Acceptance: `WatchWorkQueue` — the queue itself, live.
//!
//! A view watching a queue sees every write to it, whoever makes it: a write straight to the queue's
//! record (any process — here a store handle the daemon never built), one of the daemon's own
//! mutation RPCs, and an agent's tool call through `ExecuteTool`. The first snapshot is the queue as
//! it stands, so a reconnect resyncs; revisions only grow; a removed queue ends the watch.

mod support;

use std::time::Duration;

use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use serde_json::json;
use support::{
    a_host, a_host_as_current_user, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT,
};
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_core::session_work_queue::{SessionWorkQueues, WORK_QUEUE_FILE};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_service::proto::work_queue::{
    AddWorkItemRequest, WatchWorkQueueRequest, WorkQueueAddress, WorkQueueService,
    WorkQueueSnapshot,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_work_queue::rpc::WorkQueueWatchStream;

const OWNER: &str = "queue-owner-watch";

/// How long a write may take to reach a watcher before the test calls it lost.
const DELIVERY: Duration = Duration::from_secs(10);

fn an_item(id: &str, title: &str) -> WorkItem {
    WorkItem {
        node_id: id.to_string(),
        title: title.to_string(),
        ..WorkItem::default()
    }
}

/// `owner`'s queue on `host`, holding `n1` "Wire it", and its address.
fn a_queue_owned_by(host: &Host, owner: &str) -> WorkQueueAddress {
    let queues = the_queues_of(host);
    queues
        .create_for_session(owner, THIS_PROJECT, 0)
        .expect("the owner gets its queue");
    queues
        .update(owner, |queue| {
            *queue = WorkQueue {
                version: 1,
                nodes: vec![an_item("n1", "Wire it")],
                ..Default::default()
            }
        })
        .expect("the queue is filled");
    WorkQueueAddress {
        project_id: THIS_PROJECT.to_string(),
        queue_id: owner.to_string(),
        daemon_instance_id: HOST_A.to_string(),
    }
}

/// A handle on `host`'s queues the daemon never built — how any other writer reaches the record.
fn the_queues_of(host: &Host) -> SessionWorkQueues {
    SessionWorkQueues::new(host.sessions.path().join("sessions"))
}

async fn watch(host: &Host, address: &WorkQueueAddress) -> Result<WorkQueueWatchStream, Status> {
    host.daemon
        .watch_work_queue(Request::direct(WatchWorkQueueRequest {
            session_token: TEST_TOKEN.to_string(),
            queue: Some(address.clone()),
        }))
        .await
        .map(|r| r.into_inner())
}

async fn next_snapshot(stream: &mut WorkQueueWatchStream) -> WorkQueueSnapshot {
    tokio::time::timeout(DELIVERY, stream.next())
        .await
        .expect("a snapshot arrives in time")
        .expect("the watch is still open")
        .expect("a snapshot, not an error")
}

/// Snapshots until one carries `revision` or later — writes landing close together may arrive as
/// one snapshot of the latest.
async fn snapshot_at_or_after(
    stream: &mut WorkQueueWatchStream,
    revision: u64,
) -> WorkQueueSnapshot {
    loop {
        let snapshot = next_snapshot(stream).await;
        if snapshot.revision >= revision {
            return snapshot;
        }
    }
}

fn titles(snapshot: &WorkQueueSnapshot) -> Vec<String> {
    let queue: serde_json::Value =
        serde_json::from_str(&snapshot.work_queue_json).expect("the snapshot is JSON");
    queue["nodes"]
        .as_array()
        .map(|nodes| {
            nodes
                .iter()
                .map(|n| n["title"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn revision_inside(work_queue_json: &str) -> u64 {
    let queue: serde_json::Value = serde_json::from_str(work_queue_json).expect("JSON");
    queue["revision"].as_u64().unwrap_or_default()
}

fn revision_on_disk(host: &Host, owner: &str) -> u64 {
    the_queues_of(host)
        .read(owner)
        .expect("read")
        .expect("a queue")
        .queue
        .revision
}

#[tokio::test]
async fn the_first_snapshot_is_the_queue_as_it_stands() {
    // Given
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let address = a_queue_owned_by(&host, OWNER);

    // When
    let mut stream = watch(&host, &address).await.expect("the watch opens");
    let first = next_snapshot(&mut stream).await;

    // Then — the whole queue, at the revision it is stored at, named in and out of its JSON
    assert_eq!(
        (
            titles(&first),
            first.revision,
            revision_inside(&first.work_queue_json),
            first.summary.and_then(|s| s.address).map(|a| a.queue_id),
        ),
        (
            vec!["Wire it".to_string()],
            revision_on_disk(&host, OWNER),
            revision_on_disk(&host, OWNER),
            Some(OWNER.to_string()),
        )
    );
}

#[tokio::test]
async fn a_write_the_daemon_did_not_make_reaches_the_watch() {
    // Given — a view watching the queue
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let address = a_queue_owned_by(&host, OWNER);
    let mut stream = watch(&host, &address).await.expect("the watch opens");
    let first = next_snapshot(&mut stream).await;

    // When — another writer adds an item to the record
    let written = the_queues_of(&host)
        .update(OWNER, |queue| queue.nodes.push(an_item("n2", "Test it")))
        .expect("written");

    // Then
    let next = snapshot_at_or_after(&mut stream, written.queue.revision).await;
    assert_eq!(
        (titles(&next), next.revision),
        (
            vec!["Wire it".to_string(), "Test it".to_string()],
            first.revision + 1
        )
    );
}

#[tokio::test]
async fn a_mutation_rpc_reaches_the_watch_at_the_revision_it_answered_with() {
    // Given
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let address = a_queue_owned_by(&host, OWNER);
    let mut stream = watch(&host, &address).await.expect("the watch opens");
    next_snapshot(&mut stream).await;

    // When — another view adds an item
    let answered = host
        .daemon
        .add_work_item(Request::direct(AddWorkItemRequest {
            session_token: TEST_TOKEN.to_string(),
            stack: Some(address.clone()),
            title: "Ship it".to_string(),
            ..Default::default()
        }))
        .await
        .expect("added")
        .into_inner();

    // Then — the watch carries it, numbered as the answer was
    let answered_revision = revision_inside(&answered.work_queue_json);
    let next = snapshot_at_or_after(&mut stream, answered_revision).await;
    assert_eq!(
        (titles(&next), next.revision),
        (
            vec!["Wire it".to_string(), "Ship it".to_string()],
            answered_revision
        )
    );
}

#[tokio::test]
async fn an_agents_tool_call_reaches_the_watch() {
    // Given — a project-home session owning a queue, and a view watching it
    let host = a_host_as_current_user(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let owner = a_workspace_in(&host.daemon, THIS_PROJECT).await;
    host.make_project_home(&owner);
    let address = a_queue_owned_by(&host, &owner);
    let mut stream = watch(&host, &address).await.expect("the watch opens");
    let first = next_snapshot(&mut stream).await;

    // When — its agent adds an item with `work_item_add`
    let answered = host
        .daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: owner.clone(),
            tool_name: "work_item_add".to_string(),
            args_json: json!({ "title": "Planned by the agent", "description": "d" }).to_string(),
            daemon_instance_id: String::new(),
            conversation_id: String::new(),
        }))
        .await
        .expect("the tool answers")
        .into_inner();
    assert!(
        answered.error_message.is_empty(),
        "the tool call succeeds: {}",
        answered.error_message
    );

    // Then
    let next = snapshot_at_or_after(&mut stream, first.revision + 1).await;
    assert_eq!(
        titles(&next),
        vec!["Wire it".to_string(), "Planned by the agent".to_string()]
    );
}

#[tokio::test]
async fn writes_in_quick_succession_end_the_watch_on_the_latest_revision_in_rising_order() {
    // Given
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let address = a_queue_owned_by(&host, OWNER);
    let mut stream = watch(&host, &address).await.expect("the watch opens");
    let first = next_snapshot(&mut stream).await;

    // When — five writes, back to back
    let queues = the_queues_of(&host);
    let mut last = 0;
    for i in 2..=6 {
        last = queues
            .update(OWNER, |queue| {
                queue
                    .nodes
                    .push(an_item(&format!("n{i}"), &format!("item {i}")))
            })
            .expect("written")
            .queue
            .revision;
    }

    // Then — every snapshot is newer than the one before, and the last is the latest write
    let mut seen = vec![first.revision];
    loop {
        let snapshot = next_snapshot(&mut stream).await;
        seen.push(snapshot.revision);
        if snapshot.revision >= last {
            assert_eq!(titles(&snapshot).len(), 6);
            break;
        }
    }
    assert!(
        seen.windows(2).all(|pair| pair[0] < pair[1]),
        "revisions only rise: {seen:?}"
    );
    assert_eq!(seen.last().copied(), Some(last));
}

#[tokio::test]
async fn an_update_that_changes_nothing_sends_nothing() {
    // Given
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let address = a_queue_owned_by(&host, OWNER);
    let mut stream = watch(&host, &address).await.expect("the watch opens");
    next_snapshot(&mut stream).await;

    // When — a writer runs an update that leaves the queue as it was
    the_queues_of(&host)
        .update(OWNER, |_| {})
        .expect("answered");

    // Then — the watch stays quiet
    let quiet = tokio::time::timeout(Duration::from_millis(1500), stream.next()).await;
    assert!(quiet.is_err(), "nothing is sent: {quiet:?}");
}

#[tokio::test]
async fn a_removed_queue_ends_the_watch_with_not_found() {
    // Given
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let address = a_queue_owned_by(&host, OWNER);
    let mut stream = watch(&host, &address).await.expect("the watch opens");
    next_snapshot(&mut stream).await;

    // When — the owning session's queue goes
    std::fs::remove_file(
        host.sessions
            .path()
            .join("sessions")
            .join(OWNER)
            .join(WORK_QUEUE_FILE),
    )
    .expect("the record is removed");

    // Then
    let ended = tokio::time::timeout(DELIVERY, stream.next())
        .await
        .expect("the watch ends in time")
        .expect("an error frame before the end")
        .expect_err("the end names why");
    assert_eq!(ended.code(), Code::NotFound);
}

#[tokio::test]
async fn watching_a_queue_this_host_does_not_hold_is_not_found() {
    // Given
    let host = a_host(HOST_A, &std::sync::Arc::new(InProcessPeers::default()));
    let address = WorkQueueAddress {
        project_id: THIS_PROJECT.to_string(),
        queue_id: "nobody-owns-this".to_string(),
        daemon_instance_id: HOST_A.to_string(),
    };

    // When
    let refused = watch(&host, &address).await.expect_err("refused");

    // Then
    assert_eq!(refused.code(), Code::NotFound);
}
