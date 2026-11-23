//! PRD acceptance: PR stacks live in the project — `WorkQueueStore` creates, reads, updates and
//! lists `<projects dir>/<project_id>/stacks/<queue_id>.yaml` records, serializing writers per stack.
//!
//! Feature: docs/ft/coder/pr-stacking.md § Where a stack lives.

use std::sync::{Arc, Barrier};
use std::thread;

use pretty_assertions::assert_eq;
use tddy_changeset::changeset::{WorkItem, WorkQueue};
use tddy_changeset::work_queue_store::{WorkQueueStore, WorkQueueStoreError};

fn a_node(node_id: &str, parents: &[&str]) -> WorkItem {
    WorkItem {
        node_id: node_id.to_string(),
        title: format!("PR {node_id}"),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..WorkItem::default()
    }
}

fn a_stack(nodes: Vec<WorkItem>) -> WorkQueue {
    WorkQueue { version: 1, nodes }
}

fn node_ids(stack: &WorkQueue) -> Vec<String> {
    stack.nodes.iter().map(|n| n.node_id.clone()).collect()
}

/// A host's projects directory, empty, removed with the world.
struct AProjectsDir {
    _root: tempfile::TempDir,
    store: WorkQueueStore,
}

fn a_projects_dir() -> AProjectsDir {
    let root = tempfile::tempdir().expect("temp projects dir");
    let store = WorkQueueStore::new(root.path().join("projects"));
    AProjectsDir { _root: root, store }
}

#[test]
fn a_created_stack_is_stored_under_its_project_and_read_back() {
    // Given
    let host = a_projects_dir();
    let planned = a_stack(vec![a_node("n1", &[]), a_node("n2", &["n1"])]);

    // When
    let created = host
        .store
        .create("proj-auth", "sess-planner", planned.clone())
        .expect("create");

    // Then — the record is a file under the project's stacks/ directory, and reads back whole
    let record_path = host
        .store
        .projects_dir()
        .join("proj-auth")
        .join("stacks")
        .join(format!("{}.yaml", created.queue_id));
    assert!(
        record_path.is_file(),
        "expected the stack record at {}",
        record_path.display()
    );
    let read = host
        .store
        .read("proj-auth", &created.queue_id)
        .expect("read");
    assert_eq!(read, created);
    assert_eq!(
        (
            read.project_id.as_str(),
            read.created_by_session_id.as_str(),
            read.stack
        ),
        ("proj-auth", "sess-planner", planned)
    );
}

#[test]
fn concurrent_updates_to_one_stack_both_land() {
    // Given — one stack, and two independent store handles over the same projects dir, as two
    // RPC handlers of one daemon would hold
    let host = a_projects_dir();
    let created = host
        .store
        .create("proj-auth", "sess-planner", a_stack(vec![]))
        .expect("create");
    let writers = 8;
    let writes_per_writer = 10;
    let start = Arc::new(Barrier::new(writers));

    // When — every writer appends its own nodes at once
    let handles: Vec<_> = (0..writers)
        .map(|w| {
            let store = WorkQueueStore::new(host.store.projects_dir());
            let queue_id = created.queue_id.clone();
            let start = Arc::clone(&start);
            thread::spawn(move || {
                start.wait();
                for i in 0..writes_per_writer {
                    store
                        .update("proj-auth", &queue_id, |stack| {
                            stack.nodes.push(a_node(&format!("w{w}-n{i}"), &[]));
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
            .read("proj-auth", &created.queue_id)
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
fn a_project_id_that_escapes_the_projects_dir_is_refused() {
    // Given
    let host = a_projects_dir();

    // When
    let refused = host.store.create(
        "../outside",
        "sess-planner",
        a_stack(vec![a_node("n1", &[])]),
    );

    // Then — refused by name, and nothing was written beside the projects dir
    assert!(
        matches!(&refused, Err(WorkQueueStoreError::InvalidProjectId(id)) if id == "../outside"),
        "expected InvalidProjectId(\"../outside\"), got {refused:?}"
    );
    let parent = host.store.projects_dir().parent().unwrap();
    assert!(
        !parent.join("outside").exists(),
        "nothing may be written outside the projects dir"
    );
}

#[test]
fn listing_a_project_returns_each_of_its_stacks() {
    // Given — two stacks in one project, one in another
    let host = a_projects_dir();
    let first = host
        .store
        .create("proj-auth", "sess-a", a_stack(vec![a_node("n1", &[])]))
        .expect("create first");
    let second = host
        .store
        .create("proj-auth", "sess-b", a_stack(vec![a_node("m1", &[])]))
        .expect("create second");
    host.store
        .create("proj-billing", "sess-c", a_stack(vec![a_node("b1", &[])]))
        .expect("create in another project");

    // When
    let listed = host.store.list("proj-auth").expect("list");

    // Then — exactly that project's two stacks, sorted by stack id
    let mut expected = vec![first.queue_id, second.queue_id];
    expected.sort();
    let listed_ids: Vec<String> = listed.into_iter().map(|r| r.queue_id).collect();
    assert_eq!(listed_ids, expected);
}

#[test]
fn a_titled_stack_keeps_its_title_through_create_read_and_update() {
    // Given
    let host = a_projects_dir();

    // When — a titled stack is created, updated, then read back
    let created = host
        .store
        .create_titled(
            "proj-auth",
            "sess-planner",
            Some("Orchestrator sessions".to_string()),
            a_stack(vec![a_node("n1", &[])]),
        )
        .expect("create");
    host.store
        .update("proj-auth", &created.queue_id, |stack| {
            stack.nodes.push(a_node("n2", &["n1"]))
        })
        .expect("update");
    let read = host
        .store
        .read("proj-auth", &created.queue_id)
        .expect("read");

    // Then — the title was stored, and an update of the stack leaves it alone
    assert_eq!(
        (created.title.as_deref(), read.title.as_deref()),
        (Some("Orchestrator sessions"), Some("Orchestrator sessions"))
    );
}

#[test]
fn a_record_written_without_a_title_still_reads() {
    // Given — a record as the store wrote it before stacks had titles
    let host = a_projects_dir();
    let stacks_dir = host.store.projects_dir().join("proj-auth").join("stacks");
    std::fs::create_dir_all(&stacks_dir).unwrap();
    std::fs::write(
        stacks_dir.join("stack-old.yaml"),
        "stack_id: stack-old\nproject_id: proj-auth\ncreated_by_session_id: sess-planner\ncreated_at_unix_ms: 1\nstack:\n  version: 1\n  nodes: []\n",
    )
    .unwrap();

    // When
    let read = host.store.read("proj-auth", "stack-old").expect("read");

    // Then — it reads, untitled
    assert_eq!((read.queue_id.as_str(), read.title), ("stack-old", None));
}

/// `#workqueue` 3/14 guard: the rename keeps the on-disk spelling — a record written before it (the
/// `stacks/` directory, the record and node keys) still loads after it. Green renames the types this
/// test names; the YAML below must not change.
#[test]
fn a_record_written_before_the_rename_still_loads() {
    // Given — a record exactly as a pre-rename daemon wrote it
    let root = tempfile::tempdir().expect("temp projects dir");
    let stacks = root.path().join("projects").join("proj-1").join("stacks");
    std::fs::create_dir_all(&stacks).unwrap();
    std::fs::write(
        stacks.join("0190-stack.yaml"),
        "stack_id: 0190-stack\n\
         project_id: proj-1\n\
         created_by_session_id: sess-1\n\
         created_at_unix_ms: 1795000000000\n\
         title: The plan\n\
         stack:\n  version: 1\n  nodes:\n  - node_id: n1\n    title: First\n    parents: []\n  - node_id: n2\n    title: Second\n    parents: [n1]\n",
    )
    .unwrap();
    let store = WorkQueueStore::new(root.path().join("projects"));

    // When
    let record = store
        .read("proj-1", "0190-stack")
        .expect("the old record loads");

    // Then
    assert_eq!(
        (record.title.as_deref(), node_ids(&record.stack)),
        (Some("The plan"), vec!["n1".to_string(), "n2".to_string()])
    );
}
