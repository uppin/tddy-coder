//! Work queues addressed by id (`#workqueue` 5/14): `WorkQueueStore` reads, updates and lists the queues
//! the sessions of a host own — a queue's id is its owning session's — serializing writers per queue.
//! It creates nothing: a queue exists only because its session was started with one.
//!
//! Feature: docs/ft/coder/pr-stacking.md § Where a stack lives.

use std::sync::{Arc, Barrier};
use std::thread;

use pretty_assertions::assert_eq;
use tddy_changeset::changeset::{WorkItem, WorkQueue};
use tddy_changeset::session_work_queue::SessionWorkQueues;
use tddy_changeset::work_queue_store::{WorkQueueStore, WorkQueueStoreError};

fn a_node(node_id: &str, parents: &[&str]) -> WorkItem {
    WorkItem {
        node_id: node_id.to_string(),
        title: format!("PR {node_id}"),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..WorkItem::default()
    }
}

fn node_ids(stack: &WorkQueue) -> Vec<String> {
    stack.nodes.iter().map(|n| n.node_id.clone()).collect()
}

/// A host's data directory, removed with the world.
struct AHost {
    root: tempfile::TempDir,
    store: WorkQueueStore,
}

fn a_host() -> AHost {
    let root = tempfile::tempdir().expect("temp data dir");
    let store = WorkQueueStore::new(root.path());
    AHost { root, store }
}

impl AHost {
    /// `owner` is started with a queue of `project`.
    fn starts_with_a_queue(&self, owner: &str, project: &str) {
        SessionWorkQueues::new(self.root.path().join("sessions"))
            .create_for_session(owner, project, 7)
            .expect("created");
    }
}

#[test]
fn a_queue_is_read_back_by_its_owners_id_and_project() {
    // Given
    let host = a_host();
    host.starts_with_a_queue("sess-owner", "proj-auth");
    host.store
        .update("proj-auth", "sess-owner", |queue| {
            queue.nodes = vec![a_node("n1", &[]), a_node("n2", &["n1"])];
        })
        .expect("update");

    // When
    let read = host.store.read("proj-auth", "sess-owner").expect("read");

    // Then
    assert_eq!(
        (
            read.queue_id.as_str(),
            read.owner_session_id(),
            node_ids(&read.stack)
        ),
        (
            "sess-owner",
            "sess-owner",
            vec!["n1".to_string(), "n2".to_string()]
        )
    );
}

#[test]
fn a_queue_of_another_project_or_a_session_without_one_is_not_found() {
    // Given
    let host = a_host();
    host.starts_with_a_queue("sess-owner", "proj-auth");

    // When
    let other_project = host.store.read("proj-other", "sess-owner");
    let no_queue = host.store.read("proj-auth", "sess-plain");
    let update_of_other_project = host.store.update("proj-other", "sess-owner", |_| {});

    // Then
    assert!(matches!(
        other_project,
        Err(WorkQueueStoreError::NotFound { .. })
    ));
    assert!(matches!(
        no_queue,
        Err(WorkQueueStoreError::NotFound { .. })
    ));
    assert!(matches!(
        update_of_other_project,
        Err(WorkQueueStoreError::NotFound { .. })
    ));
}

#[test]
fn concurrent_updates_to_one_queue_both_land() {
    // Given — one queue, and independent store handles over the same data dir, as two RPC handlers
    // of one daemon would hold
    let host = a_host();
    host.starts_with_a_queue("sess-owner", "proj-auth");
    let writers = 8;
    let writes_per_writer = 10;
    let start = Arc::new(Barrier::new(writers));

    // When — every writer appends its own nodes at once
    let handles: Vec<_> = (0..writers)
        .map(|w| {
            let store = WorkQueueStore::new(host.root.path());
            let start = Arc::clone(&start);
            thread::spawn(move || {
                start.wait();
                for i in 0..writes_per_writer {
                    store
                        .update("proj-auth", "sess-owner", |queue| {
                            queue.nodes.push(a_node(&format!("w{w}-n{i}"), &[]));
                        })
                        .expect("update");
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("writer thread");
    }

    // Then — no write was lost to another
    let mut landed = node_ids(
        &host
            .store
            .read("proj-auth", "sess-owner")
            .expect("read")
            .stack,
    );
    landed.sort();
    let mut expected: Vec<String> = (0..writers)
        .flat_map(|w| (0..writes_per_writer).map(move |i| format!("w{w}-n{i}")))
        .collect();
    expected.sort();
    assert_eq!(landed, expected);
}

#[test]
fn a_project_id_that_escapes_the_data_dir_is_refused() {
    // Given
    let host = a_host();

    // When
    let read = host.store.read("../outside", "sess-owner");
    let list = host.store.list("../outside");

    // Then
    assert!(
        matches!(&read, Err(WorkQueueStoreError::InvalidProjectId(id)) if id == "../outside"),
        "{read:?}"
    );
    assert!(matches!(
        list,
        Err(WorkQueueStoreError::InvalidProjectId(_))
    ));
}

#[test]
fn listing_a_project_returns_each_of_its_queues_sorted_by_owner() {
    // Given
    let host = a_host();
    host.starts_with_a_queue("sess-b", "proj-auth");
    host.starts_with_a_queue("sess-a", "proj-auth");
    host.starts_with_a_queue("sess-c", "proj-other");

    // When
    let listed: Vec<String> = host
        .store
        .list("proj-auth")
        .expect("list")
        .into_iter()
        .map(|record| record.queue_id)
        .collect();

    // Then
    assert_eq!(listed, vec!["sess-a".to_string(), "sess-b".to_string()]);
    assert_eq!(host.store.list("proj-empty").expect("list"), Vec::new());
}
