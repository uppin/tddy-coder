//! PRD acceptance: PR stacks live in the project — `ProjectStackStore` creates, reads, updates and
//! lists `<projects dir>/<project_id>/stacks/<stack_id>.yaml` records, serializing writers per stack.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-project-stack-store.md.

use std::sync::{Arc, Barrier};
use std::thread;

use pretty_assertions::assert_eq;
use tddy_changeset::changeset::{Stack, StackNode};
use tddy_changeset::project_stack_store::{ProjectStackStore, StackStoreError};

fn a_node(node_id: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: format!("PR {node_id}"),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..StackNode::default()
    }
}

fn a_stack(nodes: Vec<StackNode>) -> Stack {
    Stack { version: 1, nodes }
}

fn node_ids(stack: &Stack) -> Vec<String> {
    stack.nodes.iter().map(|n| n.node_id.clone()).collect()
}

/// A host's projects directory, empty, removed with the world.
struct AProjectsDir {
    _root: tempfile::TempDir,
    store: ProjectStackStore,
}

fn a_projects_dir() -> AProjectsDir {
    let root = tempfile::tempdir().expect("temp projects dir");
    let store = ProjectStackStore::new(root.path().join("projects"));
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
        .join(format!("{}.yaml", created.stack_id));
    assert!(
        record_path.is_file(),
        "expected the stack record at {}",
        record_path.display()
    );
    let read = host
        .store
        .read("proj-auth", &created.stack_id)
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
            let store = ProjectStackStore::new(host.store.projects_dir());
            let stack_id = created.stack_id.clone();
            let start = Arc::clone(&start);
            thread::spawn(move || {
                start.wait();
                for i in 0..writes_per_writer {
                    store
                        .update("proj-auth", &stack_id, |stack| {
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
            .read("proj-auth", &created.stack_id)
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
        matches!(&refused, Err(StackStoreError::InvalidProjectId(id)) if id == "../outside"),
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
    let mut expected = vec![first.stack_id, second.stack_id];
    expected.sort();
    let listed_ids: Vec<String> = listed.into_iter().map(|r| r.stack_id).collect();
    assert_eq!(listed_ids, expected);
}
