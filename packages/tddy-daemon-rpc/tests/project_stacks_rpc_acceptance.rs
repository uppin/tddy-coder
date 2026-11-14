//! Acceptance: a project's PR stacks are listed across hosts and acted on by their address
//! (`#orchestrators` 9/11).
//!
//! A stack is a record of its project's store on one host (`ProjectStackStore` under that host's
//! `projects/`), so `ListProjectStacks` asks every host that may hold the project, and every other
//! `PrStackService` request names the stack it acts on by `StackAddress` — served by the host the
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
    a_host, Host, InProcessPeers, HOST_A, HOST_B, OTHER_PROJECT, PR_STACK_SERVICE, SILENT_HOST,
    THIS_PROJECT,
};
use tddy_core::changeset::{write_changeset, Changeset, Stack, StackNode};
use tddy_core::project_stack_store::{ProjectStackStore, StackRecord, StackStoreError};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{
    read_session_metadata, write_initial_tool_session_metadata, write_session_metadata,
    InitialToolSessionMetadataOpts,
};
use tddy_rpc::{Request, Status};
use tddy_service::proto::pr_stack::{
    AddPlannedPrRequest, AddPlannedPrResponse, GetProjectStackRequest, ListProjectStacksRequest,
    ListProjectStacksResponse, PrStackService, ProjectStackSummary, StackAddress,
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
fn a_stack_of(titles: &[&str]) -> Stack {
    Stack {
        version: 1,
        nodes: titles
            .iter()
            .enumerate()
            .map(|(i, title)| StackNode {
                node_id: format!("n{}", i + 1),
                title: (*title).to_string(),
                ..StackNode::default()
            })
            .collect(),
    }
}

/// What a test reads and seeds on one host's own disk.
trait HostStacks {
    fn stack_store(&self) -> ProjectStackStore;
    /// A stack of `project_id` in this host's store, planned by a session nobody lists.
    fn holding_a_stack(&self, project_id: &str, titles: &[&str]) -> StackRecord;
    /// An orchestrator-role session of `record`'s project whose changeset names `record` — and no
    /// `pr-stack` recipe, since nothing keys on a recipe any more.
    fn with_an_orchestrator_naming(&self, session_id: &str, record: &StackRecord);
    fn titles_of(&self, project_id: &str, stack_id: &str) -> Vec<String>;
}

impl HostStacks for Host {
    fn stack_store(&self) -> ProjectStackStore {
        ProjectStackStore::new(self.sessions.path().join("projects"))
    }

    fn holding_a_stack(&self, project_id: &str, titles: &[&str]) -> StackRecord {
        self.stack_store()
            .create(project_id, "a-planner-nobody-lists", a_stack_of(titles))
            .expect("seed a stack in the host's store")
    }

    fn with_an_orchestrator_naming(&self, session_id: &str, record: &StackRecord) {
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
                stack_id: Some(record.stack_id.clone()),
                ..Changeset::default()
            },
        )
        .expect("write changeset.yaml");
    }

    fn titles_of(&self, project_id: &str, stack_id: &str) -> Vec<String> {
        self.stack_store()
            .read(project_id, stack_id)
            .expect("the stack is in this host's store")
            .stack
            .nodes
            .into_iter()
            .map(|node| node.title)
            .collect()
    }
}

/// The address of `record`, held by `host`.
fn the_address_of(record: &StackRecord, host: &str) -> StackAddress {
    StackAddress {
        project_id: record.project_id.clone(),
        stack_id: record.stack_id.clone(),
        daemon_instance_id: host.to_string(),
    }
}

// ── the calls ────────────────────────────────────────────────────────────────────────────────

async fn list_project_stacks(
    host: &Host,
    project_id: &str,
) -> Result<ListProjectStacksResponse, Status> {
    host.daemon
        .list_project_stacks(Request::direct(ListProjectStacksRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project_id.to_string(),
            this_host_only: false,
        }))
        .await
        .map(|r| r.into_inner())
}

/// Append a planned PR titled `title` to the stack at `address`, asking `host`.
async fn add_planned_pr_by_address(
    host: &Host,
    address: StackAddress,
    title: &str,
) -> Result<AddPlannedPrResponse, Status> {
    host.daemon
        .add_planned_pr(Request::direct(AddPlannedPrRequest {
            session_token: TEST_TOKEN.to_string(),
            title: title.to_string(),
            stack: Some(address),
            ..Default::default()
        }))
        .await
        .map(|r| r.into_inner())
}

/// Append a planned PR titled `title` to the stack `session_id`'s changeset names, asking `host`.
async fn add_planned_pr_by_session(
    host: &Host,
    session_id: &str,
    title: &str,
) -> Result<AddPlannedPrResponse, Status> {
    host.daemon
        .add_planned_pr(Request::direct(AddPlannedPrRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            title: title.to_string(),
            ..Default::default()
        }))
        .await
        .map(|r| r.into_inner())
}

/// The node titles of the stack at `address`, read by asking `host`.
async fn titles_served_by(host: &Host, address: StackAddress) -> Result<Vec<String>, Status> {
    let answer = host
        .daemon
        .get_project_stack(Request::direct(GetProjectStackRequest {
            session_token: TEST_TOKEN.to_string(),
            stack: Some(address),
        }))
        .await?
        .into_inner();
    Ok(titles_in_plan_json(&answer.stack_plan_json))
}

fn titles_in_plan_json(stack_plan_json: &str) -> Vec<String> {
    let plan: serde_json::Value =
        serde_json::from_str(stack_plan_json).expect("stack_plan_json is JSON");
    plan["nodes"]
        .as_array()
        .expect("a plan lists its nodes")
        .iter()
        .map(|node| node["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

// ── reading a listing ────────────────────────────────────────────────────────────────────────

/// Every listed stack as `(host, stack id)`, sorted.
fn listed_addresses(listing: &ListProjectStacksResponse) -> Vec<(String, String)> {
    let mut listed: Vec<(String, String)> = listing
        .stacks
        .iter()
        .map(|summary| {
            let address = summary.address.clone().unwrap_or_default();
            (address.daemon_instance_id, address.stack_id)
        })
        .collect();
    listed.sort();
    listed
}

fn the_summary_of<'a>(
    listing: &'a ListProjectStacksResponse,
    record: &StackRecord,
) -> &'a ProjectStackSummary {
    listing
        .stacks
        .iter()
        .find(|summary| {
            summary
                .address
                .as_ref()
                .is_some_and(|a| a.stack_id == record.stack_id)
        })
        .unwrap_or_else(|| panic!("stack {} is not listed: {listing:?}", record.stack_id))
}

fn unreachable_hosts(listing: &ListProjectStacksResponse) -> Vec<String> {
    listing
        .unreachable_hosts
        .iter()
        .map(|host| host.daemon_instance_id.clone())
        .collect()
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_project_stacks_lists_every_stack_of_the_project_on_every_host() {
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
    let listing = list_project_stacks(&world.a, THIS_PROJECT)
        .await
        .expect("ListProjectStacks answers");

    // Then — each stack once, under the host that holds it
    let mut expected = vec![
        (HOST_A.to_string(), on_a.stack_id.clone()),
        (HOST_B.to_string(), on_b.stack_id.clone()),
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
    let listing = list_project_stacks(&world.a, THIS_PROJECT)
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
    assert!(listed.contains(&(HOST_A.to_string(), on_a.stack_id.clone())));
    assert!(listed.contains(&(HOST_B.to_string(), on_b.stack_id.clone())));
}

#[tokio::test]
async fn a_planned_pr_added_by_stack_address_lands_in_that_stack() {
    // Given — a stack no session names, addressed on the host that holds it
    let world = two_hosts_in_one_common_room();
    let on_a = world.a.holding_a_stack(THIS_PROJECT, &["Add token store"]);

    // When
    let added = add_planned_pr_by_address(
        &world.a,
        the_address_of(&on_a, HOST_A),
        "Add auth middleware",
    )
    .await
    .expect("AddPlannedPr by stack address answers");

    // Then — the store holds the new node, and the answer names it and carries the plan
    assert_eq!(
        world.a.titles_of(THIS_PROJECT, &on_a.stack_id),
        vec!["Add token store", "Add auth middleware"]
    );
    assert!(!added.node_id.is_empty(), "the added node is named");
    assert_eq!(
        titles_in_plan_json(&added.stack_plan_json),
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
    add_planned_pr_by_address(&world.a, address.clone(), "Add logout")
        .await
        .expect("AddPlannedPr addressed at B answers through A");

    // Then — B's store took the node, A's holds no such stack, and A forwarded rather than answered
    assert_eq!(
        world.b.titles_of(THIS_PROJECT, &on_b.stack_id),
        vec!["Add login screen", "Add logout"]
    );
    assert!(matches!(
        world.a.stack_store().read(THIS_PROJECT, &on_b.stack_id),
        Err(StackStoreError::NotFound { .. })
    ));
    assert_eq!(
        world
            .peers
            .times_asked(HOST_B, PR_STACK_SERVICE, "AddPlannedPr"),
        1
    );
    assert_eq!(
        titles_served_by(&world.a, address)
            .await
            .expect("GetProjectStack addressed at B answers through A"),
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
    add_planned_pr_by_session(&world.a, ORCHESTRATOR_ON_A, "Add auth middleware")
        .await
        .expect("AddPlannedPr by session id resolves the stack its changeset names");

    // Then
    assert_eq!(
        world.a.titles_of(THIS_PROJECT, &on_a.stack_id),
        vec!["Add token store", "Add auth middleware"]
    );
}
