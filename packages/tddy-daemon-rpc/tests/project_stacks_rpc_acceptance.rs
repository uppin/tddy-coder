//! Acceptance: a project's PR stacks are listed across hosts and acted on by their address
//! (`#orchestrators` 9/11).
//!
//! A stack is a record of its project's store on one host (`WorkQueueStore` under that host's
//! `projects/`), so `ListProjectWorkQueues` asks every host that may hold the project, and every other
//! `WorkQueueService` request names the stack it acts on by `WorkQueueAddress` — served by the host the
//! address names, forwarded there by any other. An orchestrator `session_id` still works, resolved
//! through the stack its changeset names.
//!
//! Stacks are seeded straight into each host's store, so nothing here needs a stack tool. Cross-host
//! calls go through `support`'s in-memory peer forwarder, never LiveKit; `host-c` is on every
//! roster with no daemon behind it, so it never answers.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md
//! Changeset: docs/dev/1-WIP/2026-11-10-stack-web-by-id.md

mod support;

use std::sync::Arc;

use pretty_assertions::assert_eq;
use support::{
    a_host, Host, InProcessPeers, HOST_A, HOST_B, OTHER_PROJECT, SILENT_HOST, THIS_PROJECT,
    WORK_QUEUE_SERVICE,
};
use tddy_core::changeset::{write_changeset, Changeset, WorkItem, WorkQueue};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{
    read_session_metadata, write_initial_tool_session_metadata, write_session_metadata,
    InitialToolSessionMetadataOpts,
};
use tddy_core::work_queue_store::{WorkQueueRecord, WorkQueueStore, WorkQueueStoreError};
use tddy_rpc::{Request, Status};
use tddy_service::proto::work_queue::{
    AddWorkItemRequest, AddWorkItemResponse, GetWorkQueueRequest, ListProjectWorkQueuesRequest,
    ListProjectWorkQueuesResponse, WorkQueueAddress, WorkQueueService, WorkQueueSummary,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

/// The orchestrator on host A whose changeset names A's stack.
const ORCHESTRATOR_ON_A: &str = "0190b000-0000-7000-8000-00000000000a";

// ── the world ────────────────────────────────────────────────────────────────────────────────

/// Host A and host B, each holding both projects, reaching each other through the in-process
/// peers; `host-c` is on the roster and never answers.
struct TwoHosts {
    peers: Arc<InProcessPeers>,
    a: Host,
    b: Host,
}

fn two_hosts_in_one_common_room() -> TwoHosts {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    TwoHosts { peers, a, b }
}

/// A stack whose nodes are titled `titles`, roots one after another.
fn a_stack_of(titles: &[&str]) -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: titles
            .iter()
            .enumerate()
            .map(|(i, title)| WorkItem {
                node_id: format!("n{}", i + 1),
                title: (*title).to_string(),
                ..WorkItem::default()
            })
            .collect(),
    }
}

/// What a test reads and seeds on one host's own disk.
trait HostStacks {
    fn stack_store(&self) -> WorkQueueStore;
    /// A stack of `project_id` in this host's store, planned by a session nobody lists.
    fn holding_a_stack(&self, project_id: &str, titles: &[&str]) -> WorkQueueRecord;
    /// An orchestrator-role session of `record`'s project whose changeset names `record` — and no
    /// `pr-stack` recipe, since nothing keys on a recipe any more.
    fn with_an_orchestrator_naming(&self, session_id: &str, record: &WorkQueueRecord);
    fn titles_of(&self, project_id: &str, queue_id: &str) -> Vec<String>;
}

impl HostStacks for Host {
    fn stack_store(&self) -> WorkQueueStore {
        WorkQueueStore::new(self.sessions.path().join("projects"))
    }

    fn holding_a_stack(&self, project_id: &str, titles: &[&str]) -> WorkQueueRecord {
        self.stack_store()
            .create(project_id, "a-planner-nobody-lists", a_stack_of(titles))
            .expect("seed a stack in the host's store")
    }

    fn with_an_orchestrator_naming(&self, session_id: &str, record: &WorkQueueRecord) {
        let dir = unified_session_dir_path(self.sessions.path(), session_id);
        std::fs::create_dir_all(&dir).expect("session dir");
        write_initial_tool_session_metadata(
            &dir,
            InitialToolSessionMetadataOpts {
                project_id: record.project_id.clone(),
                ..InitialToolSessionMetadataOpts::default()
            },
        )
        .expect("write .session.yaml");
        let mut meta = read_session_metadata(&dir).expect("read .session.yaml");
        meta.session_role = Some("orchestrator".to_string());
        write_session_metadata(&dir, &meta).expect("write the orchestrator role");
        write_changeset(
            &dir,
            &Changeset {
                work_queue_id: Some(record.queue_id.clone()),
                ..Changeset::default()
            },
        )
        .expect("write changeset.yaml");
    }

    fn titles_of(&self, project_id: &str, queue_id: &str) -> Vec<String> {
        self.stack_store()
            .read(project_id, queue_id)
            .expect("the stack is in this host's store")
            .stack
            .nodes
            .into_iter()
            .map(|node| node.title)
            .collect()
    }
}

/// The address of `record`, held by `host`.
fn the_address_of(record: &WorkQueueRecord, host: &str) -> WorkQueueAddress {
    WorkQueueAddress {
        project_id: record.project_id.clone(),
        queue_id: record.queue_id.clone(),
        daemon_instance_id: host.to_string(),
    }
}

// ── the calls ────────────────────────────────────────────────────────────────────────────────

async fn list_project_work_queues(
    host: &Host,
    project_id: &str,
) -> Result<ListProjectWorkQueuesResponse, Status> {
    host.daemon
        .list_project_work_queues(Request::direct(ListProjectWorkQueuesRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project_id.to_string(),
            this_host_only: false,
        }))
        .await
        .map(|r| r.into_inner())
}

/// Append a planned PR titled `title` to the stack at `address`, asking `host`.
async fn add_work_item_by_address(
    host: &Host,
    address: WorkQueueAddress,
    title: &str,
) -> Result<AddWorkItemResponse, Status> {
    host.daemon
        .add_work_item(Request::direct(AddWorkItemRequest {
            session_token: TEST_TOKEN.to_string(),
            title: title.to_string(),
            stack: Some(address),
            ..Default::default()
        }))
        .await
        .map(|r| r.into_inner())
}

/// Append a planned PR titled `title` to the stack `session_id`'s changeset names, asking `host`.
async fn add_work_item_by_session(
    host: &Host,
    session_id: &str,
    title: &str,
) -> Result<AddWorkItemResponse, Status> {
    host.daemon
        .add_work_item(Request::direct(AddWorkItemRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            title: title.to_string(),
            ..Default::default()
        }))
        .await
        .map(|r| r.into_inner())
}

/// The node titles of the stack at `address`, read by asking `host`.
async fn titles_served_by(host: &Host, address: WorkQueueAddress) -> Result<Vec<String>, Status> {
    let answer = host
        .daemon
        .get_work_queue(Request::direct(GetWorkQueueRequest {
            session_token: TEST_TOKEN.to_string(),
            stack: Some(address),
        }))
        .await?
        .into_inner();
    Ok(titles_in_plan_json(&answer.work_queue_json))
}

fn titles_in_plan_json(work_queue_json: &str) -> Vec<String> {
    let plan: serde_json::Value =
        serde_json::from_str(work_queue_json).expect("work_queue_json is JSON");
    plan["nodes"]
        .as_array()
        .expect("a plan lists its nodes")
        .iter()
        .map(|node| node["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

// ── reading a listing ────────────────────────────────────────────────────────────────────────

/// Every listed stack as `(host, stack id)`, sorted.
fn listed_addresses(listing: &ListProjectWorkQueuesResponse) -> Vec<(String, String)> {
    let mut listed: Vec<(String, String)> = listing
        .stacks
        .iter()
        .map(|summary| {
            let address = summary.address.clone().unwrap_or_default();
            (address.daemon_instance_id, address.queue_id)
        })
        .collect();
    listed.sort();
    listed
}

fn the_summary_of<'a>(
    listing: &'a ListProjectWorkQueuesResponse,
    record: &WorkQueueRecord,
) -> &'a WorkQueueSummary {
    listing
        .stacks
        .iter()
        .find(|summary| {
            summary
                .address
                .as_ref()
                .is_some_and(|a| a.queue_id == record.queue_id)
        })
        .unwrap_or_else(|| panic!("stack {} is not listed: {listing:?}", record.queue_id))
}

fn unreachable_hosts(listing: &ListProjectWorkQueuesResponse) -> Vec<String> {
    listing
        .unreachable_hosts
        .iter()
        .map(|host| host.daemon_instance_id.clone())
        .collect()
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_project_work_queues_lists_every_stack_of_the_project_on_every_host() {
    // Given — a stack of the project on each host, an orchestrator on A naming A's, and a stack of
    // another project on A that must not be listed
    let world = two_hosts_in_one_common_room();
    let on_a = world
        .a
        .holding_a_stack(THIS_PROJECT, &["Add token store", "Add auth middleware"]);
    let on_b = world.b.holding_a_stack(THIS_PROJECT, &["Add login screen"]);
    world.a.holding_a_stack(OTHER_PROJECT, &["Unrelated work"]);
    world
        .a
        .with_an_orchestrator_naming(ORCHESTRATOR_ON_A, &on_a);

    // When
    let listing = list_project_work_queues(&world.a, THIS_PROJECT)
        .await
        .expect("ListProjectWorkQueues answers");

    // Then — each stack once, under the host that holds it
    let mut expected = vec![
        (HOST_A.to_string(), on_a.queue_id.clone()),
        (HOST_B.to_string(), on_b.queue_id.clone()),
    ];
    expected.sort();
    assert_eq!(listed_addresses(&listing), expected);
    let summary = the_summary_of(&listing, &on_a);
    assert_eq!(
        summary.address.as_ref().map(|a| a.project_id.as_str()),
        Some(THIS_PROJECT)
    );
    assert_eq!(summary.node_counts.as_ref().map(|c| c.total), Some(2));
    assert!(
        summary
            .referencing_session_ids
            .iter()
            .any(|id| id == ORCHESTRATOR_ON_A),
        "the orchestrator naming the stack must be listed as referencing it: {summary:?}"
    );
}

#[tokio::test]
async fn an_unreachable_host_is_named_not_silently_dropped() {
    // Given — host-c is eligible and never answers
    let world = two_hosts_in_one_common_room();
    let on_a = world.a.holding_a_stack(THIS_PROJECT, &["Add token store"]);
    let on_b = world.b.holding_a_stack(THIS_PROJECT, &["Add login screen"]);

    // When
    let listing = list_project_work_queues(&world.a, THIS_PROJECT)
        .await
        .expect("a host that does not answer fails the listing of none of the others");

    // Then — the silent host is named; the hosts that answered are listed and not named
    let unreachable = unreachable_hosts(&listing);
    assert!(
        unreachable.iter().any(|host| host == SILENT_HOST),
        "{SILENT_HOST} must be named as unreachable: {unreachable:?}"
    );
    assert!(
        !unreachable.iter().any(|host| host == HOST_B),
        "{HOST_B} answered and must not be named as unreachable: {unreachable:?}"
    );
    let listed = listed_addresses(&listing);
    assert!(listed.contains(&(HOST_A.to_string(), on_a.queue_id.clone())));
    assert!(listed.contains(&(HOST_B.to_string(), on_b.queue_id.clone())));
}

#[tokio::test]
async fn a_planned_pr_added_by_stack_address_lands_in_that_stack() {
    // Given — a stack no session names, addressed on the host that holds it
    let world = two_hosts_in_one_common_room();
    let on_a = world.a.holding_a_stack(THIS_PROJECT, &["Add token store"]);

    // When
    let added = add_work_item_by_address(
        &world.a,
        the_address_of(&on_a, HOST_A),
        "Add auth middleware",
    )
    .await
    .expect("AddWorkItem by stack address answers");

    // Then — the store holds the new node, and the answer names it and carries the plan
    assert_eq!(
        world.a.titles_of(THIS_PROJECT, &on_a.queue_id),
        vec!["Add token store", "Add auth middleware"]
    );
    assert!(!added.item_id.is_empty(), "the added node is named");
    assert_eq!(
        titles_in_plan_json(&added.work_queue_json),
        vec!["Add token store", "Add auth middleware"]
    );
}

#[tokio::test]
async fn a_stack_address_naming_another_host_is_served_there() {
    // Given — the stack lives on B; the caller asks A
    let world = two_hosts_in_one_common_room();
    let on_b = world.b.holding_a_stack(THIS_PROJECT, &["Add login screen"]);
    let address = the_address_of(&on_b, HOST_B);

    // When
    add_work_item_by_address(&world.a, address.clone(), "Add logout")
        .await
        .expect("AddWorkItem addressed at B answers through A");

    // Then — B's store took the node, A's holds no such stack, and A forwarded rather than answered
    assert_eq!(
        world.b.titles_of(THIS_PROJECT, &on_b.queue_id),
        vec!["Add login screen", "Add logout"]
    );
    assert!(matches!(
        world.a.stack_store().read(THIS_PROJECT, &on_b.queue_id),
        Err(WorkQueueStoreError::NotFound { .. })
    ));
    assert_eq!(
        world
            .peers
            .times_asked(HOST_B, WORK_QUEUE_SERVICE, "AddWorkItem"),
        1
    );
    assert_eq!(
        titles_served_by(&world.a, address)
            .await
            .expect("GetWorkQueue addressed at B answers through A"),
        vec!["Add login screen", "Add logout"]
    );
}

#[tokio::test]
async fn a_session_id_address_still_resolves_through_its_stack_id() {
    // Given — an orchestrator-role session, not a `pr-stack` recipe, whose changeset names a stack
    let world = two_hosts_in_one_common_room();
    let on_a = world.a.holding_a_stack(THIS_PROJECT, &["Add token store"]);
    world
        .a
        .with_an_orchestrator_naming(ORCHESTRATOR_ON_A, &on_a);

    // When
    add_work_item_by_session(&world.a, ORCHESTRATOR_ON_A, "Add auth middleware")
        .await
        .expect("AddWorkItem by session id resolves the stack its changeset names");

    // Then
    assert_eq!(
        world.a.titles_of(THIS_PROJECT, &on_a.queue_id),
        vec!["Add token store", "Add auth middleware"]
    );
}

#[tokio::test]
async fn create_work_queue_answers_an_address_that_then_takes_planned_prs() {
    // Given — two hosts in one room
    let world = two_hosts_in_one_common_room();

    // When — a stack is created by address (the web's "New stack"), with a title
    let created = world
        .a
        .daemon
        .create_work_queue(Request::direct(
            tddy_service::proto::work_queue::CreateWorkQueueRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: THIS_PROJECT.to_string(),
                title: "Auth stack".to_string(),
                daemon_instance_id: String::new(),
            },
        ))
        .await
        .expect("CreateWorkQueue on the owning host")
        .into_inner();
    let address = created.stack.expect("the answer names the new stack");

    // Then — the record is in A's store, titled, empty, and addressable
    let record = world
        .a
        .stack_store()
        .read(THIS_PROJECT, &address.queue_id)
        .expect("the created stack is in the creating host's store");
    assert_eq!(record.title.as_deref(), Some("Auth stack"));
    assert!(record.stack.nodes.is_empty());
    add_work_item_by_address(&world.a, address, "Add token store")
        .await
        .expect("a planned PR can be added to the created stack by address");
    assert_eq!(
        world.a.titles_of(THIS_PROJECT, &record.queue_id),
        vec!["Add token store"]
    );
}

#[tokio::test]
async fn create_work_queue_naming_another_host_is_created_there() {
    // Given — two hosts in one room
    let world = two_hosts_in_one_common_room();

    // When — A is asked to create a stack owned by B
    let created = world
        .a
        .daemon
        .create_work_queue(Request::direct(
            tddy_service::proto::work_queue::CreateWorkQueueRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: THIS_PROJECT.to_string(),
                title: String::new(),
                daemon_instance_id: HOST_B.to_string(),
            },
        ))
        .await
        .expect("CreateWorkQueue forwarded to the owning host")
        .into_inner();
    let address = created.stack.expect("the answer names the new stack");

    // Then — the record is in B's store, not A's
    assert!(
        world
            .b
            .stack_store()
            .read(THIS_PROJECT, &address.queue_id)
            .is_ok(),
        "the created stack lives on the host the request named"
    );
    assert!(
        world
            .a
            .stack_store()
            .read(THIS_PROJECT, &address.queue_id)
            .is_err(),
        "the created stack does not also live on the asking host"
    );
}
