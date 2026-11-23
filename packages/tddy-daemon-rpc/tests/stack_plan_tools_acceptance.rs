//! Acceptance: the PR-stack plan and read tools are daemon exec tools addressed by `queue_id`
//! (`#orchestrators` 7/11). Main creates a titled stack, plans it and reads its status from its
//! read-only session; an orchestrator's tools default to its own stack (the one its changeset
//! names); a stack another host holds is
//! read and edited through forwarding; a node of a stack is offered status and read for its own stack
//! alone; an ordinary session is offered none and refused; and a synced edit reaches GitHub as the
//! project's account.
//!
//! Stacks are the project's records (`WorkQueueStore` under each host's `projects/`), read back
//! here through the real store. GitHub is a recording stand-in attached to the daemon
//! (`RpcHandlers::attach_stack_github`); the project is a Local gh one, so its account's token is
//! what the host's `gh` stub answers. Cross-host calls go through `support`'s in-memory peer
//! forwarder, never LiveKit.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-plan-tools.md
//! Changeset: docs/dev/1-WIP/2026-11-10-stack-plan-tools.md
//!
//! `#orchestrators` 13/13 adds a node's two reads where #884 left them out: a node session in a
//! jail is listed them by the jail's own `ListExecTools` bridge (driven here through the daemon's
//! production `sandbox_rpc_handler`, with no jail spawned), and a node session on another host is
//! served them by the host that holds its stack, which finds the node bound to it in its own store.
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-14-stack-node-reads.md
//! Changeset: docs/dev/1-WIP/2026-11-14-stack-node-reads.md

mod support;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use prost::Message as _;
use serde_json::{json, Value};
use support::{
    a_host_as_current_user_with_gh_at, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B,
    THIS_PROJECT,
};
use tddy_core::changeset::{read_changeset, write_changeset, GithubPrStatus, WorkItem, WorkQueue};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_work_queue::SessionWorkQueues;
use tddy_core::work_queue_store::{WorkQueueRecord, WorkQueueStore};
use tddy_core::WorkflowError;
use tddy_daemon_rpc::exec_tool::work_queue_tools::{WorkQueueGithub, WorkQueueGithubClients};
use tddy_github::pr_api::{
    CheckRun, GithubPrApi, GithubPrInsightApi, PrDetail, PrFile, PrIssueComment, PrLookupOutcome,
    PrRef, PrReview, PrReviewComment, PrSearchHit, PrSearchQuery,
};
use tddy_projects::project_storage::{read_projects, write_projects};
use tddy_rpc::RpcResult;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
    ListExecToolsResponse,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::gh_cli::a_gh_cli;
use tddy_tool_engine::work_queue_tools::is_stack_tool;
use tddy_tool_engine::EXEC_TOOL_SERVICE;

/// The token the project's account resolves to: what the host's `gh` stub answers.
const PROJECTS_ACCOUNT_TOKEN: &str = "gho_the_projects_account";
const REPO: &str = "acme/alpha";

// ── GitHub ───────────────────────────────────────────────────────────────────────────────────

/// What the stand-in GitHub was asked: every client built, as `(repo, token)`, and every title/body
/// edit, as `(pull number, title, body)`.
#[derive(Default)]
struct GithubRecord {
    clients: Vec<(String, String)>,
    title_body_edits: Vec<(u64, Option<String>, Option<String>)>,
}

/// GitHub as the stack tools reach it: no pull request exists for any branch, and a title/body
/// edit is recorded and accepted.
#[derive(Clone, Default)]
struct RecordingGithub(Arc<Mutex<GithubRecord>>);

impl RecordingGithub {
    fn clients_built(&self) -> Vec<(String, String)> {
        self.0.lock().unwrap().clients.clone()
    }

    fn title_body_edits(&self) -> Vec<(u64, Option<String>, Option<String>)> {
        self.0.lock().unwrap().title_body_edits.clone()
    }
}

impl WorkQueueGithubClients for RecordingGithub {
    fn client_for(&self, repo: &str, token: &str) -> Box<dyn WorkQueueGithub> {
        self.0
            .lock()
            .unwrap()
            .clients
            .push((repo.to_string(), token.to_string()));
        Box::new(self.clone())
    }
}

fn not_served(what: &str) -> WorkflowError {
    WorkflowError::WriteFailed(format!("the stand-in GitHub serves no {what}"))
}

impl GithubPrApi for RecordingGithub {
    fn get_open_pr(&self, _head_branch: &str) -> Result<Option<PrRef>, WorkflowError> {
        Ok(None)
    }
    fn get_pr_by_head(&self, _head_branch: &str) -> PrLookupOutcome {
        PrLookupOutcome::NotFound
    }
    fn merge_pr(&self, _number: u64) -> Result<String, WorkflowError> {
        Err(not_served("merge"))
    }
    fn patch_pr_base(&self, _number: u64, _new_base: &str) -> Result<(), WorkflowError> {
        Err(not_served("repoint"))
    }
    fn create_pr(
        &self,
        _head: &str,
        _base: &str,
        _title: &str,
        _body: &str,
    ) -> Result<u64, WorkflowError> {
        Err(not_served("PR creation"))
    }
    fn disable_auto_merge(&self, _number: u64) -> Result<(), WorkflowError> {
        Err(not_served("auto-merge"))
    }
    fn close_pr(&self, _number: u64) -> Result<(), WorkflowError> {
        Err(not_served("close"))
    }
}

impl GithubPrInsightApi for RecordingGithub {
    fn get_pr(&self, _number: u64) -> Result<PrDetail, WorkflowError> {
        Err(not_served("PR read"))
    }
    fn list_pr_files(&self, _number: u64) -> Result<Vec<PrFile>, WorkflowError> {
        Err(not_served("file list"))
    }
    fn list_check_runs(&self, _head_sha: &str) -> Result<Vec<CheckRun>, WorkflowError> {
        Err(not_served("check runs"))
    }
    fn list_reviews(&self, _number: u64) -> Result<Vec<PrReview>, WorkflowError> {
        Err(not_served("reviews"))
    }
    fn list_review_comments(&self, _number: u64) -> Result<Vec<PrReviewComment>, WorkflowError> {
        Err(not_served("review comments"))
    }
    fn list_issue_comments(&self, _number: u64) -> Result<Vec<PrIssueComment>, WorkflowError> {
        Err(not_served("conversation"))
    }
    fn search_prs(&self, _query: &PrSearchQuery) -> Result<Vec<PrSearchHit>, WorkflowError> {
        Err(not_served("search"))
    }
    fn patch_pr_title_body(
        &self,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<(), WorkflowError> {
        self.0.lock().unwrap().title_body_edits.push((
            number,
            title.map(str::to_string),
            body.map(str::to_string),
        ));
        Ok(())
    }
}

// ── the world ────────────────────────────────────────────────────────────────────────────────

/// Two hosts of one project, each reaching the stand-in GitHub, with main on host-a. The project is
/// a GitHub project acting as the host's `gh` login (Local gh), whose stub answers
/// [`PROJECTS_ACCOUNT_TOKEN`].
struct World {
    a: Host,
    b: Host,
    peers: Arc<InProcessPeers>,
    github: RecordingGithub,
    main: String,
    _gh: tempfile::TempDir,
}

async fn a_world() -> World {
    let gh_dir = tempfile::tempdir().unwrap();
    let gh = a_gh_cli(gh_dir.path())
        .logged_in_as("acme-bot", 7)
        .with_token(PROJECTS_ACCOUNT_TOKEN)
        .build();
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host_as_current_user_with_gh_at(HOST_A, &peers, &gh);
    let b = a_host_as_current_user_with_gh_at(HOST_B, &peers, &gh);
    let github = RecordingGithub::default();
    for host in [&a, &b] {
        host.project_is_on_github_acting_as_the_hosts_gh();
        host.daemon
            .handlers()
            .attach_stack_github(Arc::new(github.clone()));
    }
    let main = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&main);
    World {
        a,
        b,
        peers,
        github,
        main,
        _gh: gh_dir,
    }
}

impl Host {
    fn session_dir(&self, session_id: &str) -> PathBuf {
        unified_session_dir_path(self.sessions.path(), session_id)
    }

    /// This host's stack store — the one the daemon serves the tools over.
    fn stacks(&self) -> WorkQueueStore {
        WorkQueueStore::new(self.sessions.path())
    }

    /// The project's row names the GitHub repository and acts as the host's `gh` login.
    fn project_is_on_github_acting_as_the_hosts_gh(&self) {
        let projects_dir = self.sessions.path().join("projects");
        let rows = read_projects(&projects_dir)
            .unwrap()
            .into_iter()
            .map(|mut row| {
                if row.project_id == THIS_PROJECT {
                    row.git_url = format!("https://github.com/{REPO}.git");
                    row.local_gh = true;
                }
                row
            })
            .collect::<Vec<_>>();
        write_projects(&projects_dir, &rows).unwrap();
    }

    /// A work queue of the project held by this host, owned by `owner` (a session of it), with
    /// `titles` as its root items `n1`, `n2`, …. A queue's id is its owner's. Answers that id.
    fn holds_a_stack(&self, owner: &str, titles: &[&str]) -> String {
        let queues = SessionWorkQueues::new(self.sessions.path().join("sessions"));
        queues
            .create_for_session(owner, THIS_PROJECT, 0)
            .expect("the owner gets its queue");
        queues
            .update(owner, |queue| *queue = a_stack_of(titles))
            .expect("the queue is filled");
        owner.to_string()
    }

    /// An orchestrator of the project on this host that owns a queue with `titles` as its items.
    /// Answers the session and its queue's id (the same).
    async fn an_orchestrator_owning_a_stack_of(&self, titles: &[&str]) -> (String, String) {
        let session_id = a_workspace_in(&self.daemon, THIS_PROJECT).await;
        let queue_id = self.holds_a_stack(&session_id, titles);
        (session_id, queue_id)
    }

    /// A session of the project on this host started by `orchestrator` for `node_id` of its stack:
    /// the node is bound to it, and its changeset names `orchestrator`.
    async fn a_node_session_of(&self, orchestrator: &str, queue_id: &str, node_id: &str) -> String {
        let session_id = a_workspace_in(&self.daemon, THIS_PROJECT).await;
        let dir = self.session_dir(&session_id);
        let mut changeset = read_changeset(&dir).unwrap_or_default();
        changeset.orchestrator_session_id = Some(orchestrator.to_string());
        write_changeset(&dir, &changeset).unwrap();
        self.stacks()
            .update(THIS_PROJECT, queue_id, |stack| {
                let node = stack
                    .nodes
                    .iter_mut()
                    .find(|n| n.node_id == node_id)
                    .expect("the node is planned");
                node.session_id = Some(session_id.clone());
            })
            .unwrap();
        session_id
    }

    /// `node_id` of `queue_id` records its open pull request, number `number`.
    fn node_has_pull_request(&self, queue_id: &str, node_id: &str, number: u64) {
        self.stacks()
            .update(THIS_PROJECT, queue_id, |stack| {
                let node = stack
                    .nodes
                    .iter_mut()
                    .find(|n| n.node_id == node_id)
                    .expect("the node is planned");
                node.branch = Some(format!("feature/alpha/{node_id}"));
                node.pr_status = Some(GithubPrStatus {
                    phase: "open".to_string(),
                    url: Some(format!("https://github.com/{REPO}/pull/{number}")),
                    error: None,
                });
            })
            .unwrap();
    }

    /// The nodes of `queue_id` as this host's store holds them, as `(node_id, title, parents)`.
    fn nodes_of(&self, queue_id: &str) -> Vec<(String, String, Vec<String>)> {
        nodes_of(&self.stacks().read(THIS_PROJECT, queue_id).unwrap())
    }

    /// Call `tool` as `session_id`, addressed at this host.
    async fn call_as(
        &self,
        session_id: &str,
        tool: &str,
        args: Value,
    ) -> Result<ExecuteToolResponse, Status> {
        self.daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    /// Call `tool` as `session_id` with the request addressed at host `addressed` — how an agent's
    /// call reaches the host that holds its stack: this host forwards it there whole.
    async fn call_addressed_at(
        &self,
        addressed: &str,
        session_id: &str,
        tool: &str,
        args: Value,
    ) -> Result<ExecuteToolResponse, Status> {
        self.daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: addressed.to_string(),
                conversation_id: String::new(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    /// The stack tools the jail built for `session_id` is listed by its `ListExecTools` bridge —
    /// the daemon's production `sandbox_rpc_handler`, relayed to as an in-jail agent's `tddy-tools`
    /// is, with no jail spawned — in the order listed.
    async fn work_queue_tools_listed_in_the_jail_of(&self, session_id: &str) -> Vec<String> {
        let service = self.daemon.as_arc();
        service.install_sandbox_rpc_bridge();
        let bridge = service.sandbox_rpc_handler(session_id, &self.session_dir(session_id));
        let asked = ListExecToolsRequest {
            session_id: session_id.to_string(),
            ..Default::default()
        };
        let RpcResult::Unary(answer) = bridge
            .handle_rpc(EXEC_TOOL_SERVICE, "ListExecTools", &asked.encode_to_vec())
            .await
        else {
            panic!("ListExecTools is unary");
        };
        let answer = answer.unwrap_or_else(|status| panic!("the jail is answered: {status:?}"));
        ListExecToolsResponse::decode(answer.as_slice())
            .expect("a ListExecToolsResponse")
            .tools
            .into_iter()
            .map(|tool| tool.name)
            .filter(|name| is_stack_tool(name))
            .collect()
    }

    /// [`Self::call_as`], which must be served without error; its JSON answer.
    async fn answer_to(&self, session_id: &str, tool: &str, args: Value) -> Value {
        let answer = self
            .call_as(session_id, tool, args)
            .await
            .unwrap_or_else(|status| panic!("{tool} must be served: {status:?}"));
        assert!(!answer.is_error, "{tool} failed: {}", answer.error_message);
        serde_json::from_str(&answer.result_json).expect("the answer is JSON")
    }

    /// The stack tools `ListExecTools` offers `session_id`, in the order offered.
    async fn work_queue_tools_offered_to(&self, session_id: &str) -> Vec<String> {
        self.daemon
            .list_exec_tools(Request::direct(ListExecToolsRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                ..Default::default()
            }))
            .await
            .expect("the session's tools are listed")
            .into_inner()
            .tools
            .into_iter()
            .map(|tool| tool.name)
            .filter(|name| is_stack_tool(name))
            .collect()
    }
}

/// A session of the project held by `session_host`, started by `orchestrator` for `node_id` of
/// `queue_id`, which `stack_host` holds: the node is bound to it in `stack_host`'s store, and its
/// changeset names `orchestrator`.
async fn a_node_session_held_by(
    session_host: &Host,
    stack_host: &Host,
    orchestrator: &str,
    queue_id: &str,
    node_id: &str,
) -> String {
    let session_id = a_workspace_in(&session_host.daemon, THIS_PROJECT).await;
    let dir = session_host.session_dir(&session_id);
    let mut changeset = read_changeset(&dir).unwrap_or_default();
    changeset.orchestrator_session_id = Some(orchestrator.to_string());
    write_changeset(&dir, &changeset).unwrap();
    stack_host
        .stacks()
        .update(THIS_PROJECT, queue_id, |stack| {
            let node = stack
                .nodes
                .iter_mut()
                .find(|n| n.node_id == node_id)
                .expect("the node is planned");
            node.session_id = Some(session_id.clone());
        })
        .unwrap();
    session_id
}

fn a_stack_of(titles: &[&str]) -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: titles
            .iter()
            .enumerate()
            .map(|(i, title)| WorkItem {
                node_id: format!("n{}", i + 1),
                title: title.to_string(),
                ..WorkItem::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn nodes_of(record: &WorkQueueRecord) -> Vec<(String, String, Vec<String>)> {
    record
        .stack
        .nodes
        .iter()
        .map(|n| (n.node_id.clone(), n.title.clone(), n.parents.clone()))
        .collect()
}

/// The `(node_id, title)` of every node a `work_queue_status` answer lists.
fn status_nodes(status: &Value) -> Vec<(String, String)> {
    status["nodes"]
        .as_array()
        .expect("a status answer lists nodes")
        .iter()
        .map(|n| {
            (
                n["node_id"].as_str().unwrap().to_string(),
                n["title"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn node(node_id: &str, title: &str, parents: &[&str]) -> (String, String, Vec<String>) {
    (
        node_id.to_string(),
        title.to_string(),
        parents.iter().map(|p| p.to_string()).collect(),
    )
}

fn pair(node_id: &str, title: &str) -> (String, String) {
    (node_id.to_string(), title.to_string())
}

const EVERY_STACK_TOOL: [&str; 26] = [
    "work_queue_list",
    "work_queue_status",
    "work_pr_read",
    "work_pr_search",
    "work_pr_comments",
    "work_item_add",
    "work_item_update",
    "work_item_delete",
    "work_item_set_status",
    "work_item_adopt",
    "work_item_assign_pr",
    "work_item_unassign_pr",
    "work_item_link_session",
    "work_item_unlink_session",
    "work_item_set_labels",
    "work_item_set_badge",
    "work_item_clear_badge",
    "work_queue_set_dependencies",
    "work_queue_graph",
    "work_queue_waves",
    "work_item_inbox",
    "work_item_outbox",
    "work_item_schedule",
    "work_queue_schedule_wave",
    "work_item_unschedule",
    "work_queue_schedules",
];

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn main_plans_on_its_own_queue_and_reads_its_status_from_a_read_only_session() {
    // Given main, a read-only session of the project, that owns a work queue (made at its start)
    let world = a_world().await;
    let a = &world.a;
    let queue_id = a.holds_a_stack(&world.main, &[]);

    // When it plans two items on it, reads its status and lists the queues, naming none
    let first = a
        .answer_to(
            &world.main,
            "work_item_add",
            json!({ "title": "Store", "description": "the store" }),
        )
        .await;
    let first_id = first["node_id"].as_str().expect("a node_id").to_string();
    a.answer_to(
        &world.main,
        "work_item_add",
        json!({
            "title": "Tools",
            "description": "the tools",
            "parents": [first_id],
        }),
    )
    .await;
    let status = a
        .answer_to(&world.main, "work_queue_status", json!({}))
        .await;
    let listed = a.answer_to(&world.main, "work_queue_list", json!({})).await;

    // Then main is offered every work tool, its queue holds both items, the status lists them, and
    // the list shows the queue by its owner's id
    assert_eq!(
        a.work_queue_tools_offered_to(&world.main).await,
        names(&EVERY_STACK_TOOL)
    );
    let record = a.stacks().read(THIS_PROJECT, &queue_id).unwrap();
    assert_eq!(record.owner_session_id(), world.main.as_str());
    assert_eq!(
        nodes_of(&record),
        vec![node("n1", "Store", &[]), node("n2", "Tools", &["n1"]),]
    );
    assert_eq!(
        status_nodes(&status),
        vec![pair("n1", "Store"), pair("n2", "Tools")]
    );
    assert_eq!(
        listed["stacks"]
            .as_array()
            .expect("a list answer lists queues")
            .iter()
            .map(|s| (s["queue_id"].clone(), s["owner_session_id"].clone()))
            .collect::<Vec<_>>(),
        vec![(json!(queue_id), json!(world.main))]
    );
}

#[tokio::test]
async fn status_carries_each_items_wave_and_binding_needs_and_the_waves_tool_answers_them() {
    // Given main's queue of three items, n2 and n3 each needing the one before
    let world = a_world().await;
    let a = &world.a;
    a.holds_a_stack(&world.main, &["Store", "Tools", "Web"]);
    a.answer_to(
        &world.main,
        "work_queue_set_dependencies",
        json!({ "item_id": "n2", "needs": ["n1"] }),
    )
    .await;
    a.answer_to(
        &world.main,
        "work_queue_set_dependencies",
        json!({ "item_id": "n3", "needs": ["n2"] }),
    )
    .await;

    // When it reads the status and the waves
    let status = a
        .answer_to(&world.main, "work_queue_status", json!({}))
        .await;
    let waves = a
        .answer_to(&world.main, "work_queue_waves", json!({}))
        .await;

    // Then each item carries its wave and what it waits on, and the waves tool agrees
    let carried: Vec<(Value, Value)> = status["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|n| (n["wave"].clone(), n["binding_needs"].clone()))
        .collect();
    assert_eq!(
        carried,
        vec![
            (json!(1), json!([])),
            (json!(2), json!(["n1"])),
            (json!(3), json!(["n2"])),
        ]
    );
    assert_eq!(waves["derived_line"], json!(["n1", "n2", "n3"]));
    assert_eq!(waves["out_of_order_at"], Value::Null);
}

#[tokio::test]
async fn a_session_whose_role_is_orchestrator_but_owns_no_queue_is_offered_no_work_tool() {
    // Given an orchestrator-role session that was never given a queue (access never reads a role)
    let world = a_world().await;
    let a = &world.a;
    let session = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let dir = a.session_dir(&session);
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.session_role = Some("orchestrator".to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();

    // When it is listed its tools and calls one
    let refusal = a
        .call_as(&session, "work_queue_list", json!({}))
        .await
        .expect_err("a session with no queue is refused the work tools");

    // Then none is offered, and the call is refused as permission denied
    assert_eq!(
        a.work_queue_tools_offered_to(&session).await,
        Vec::<String>::new()
    );
    assert_eq!(refusal.code(), Code::PermissionDenied, "{refusal:?}");
}

#[tokio::test]
async fn addressing_a_session_that_owns_no_queue_is_refused_with_the_hint() {
    // Given main, owning a queue, and an ordinary session with none
    let world = a_world().await;
    let a = &world.a;
    a.holds_a_stack(&world.main, &["Store"]);
    let plain = a_workspace_in(&a.daemon, THIS_PROJECT).await;

    // When main reads the status of the plain session's queue
    let answer = a
        .call_as(
            &world.main,
            "work_queue_status",
            json!({ "queue_id": plain }),
        )
        .await
        .expect("a no-queue refusal is the tool's answer");

    // Then it is refused, naming the session and how a queue comes to exist
    assert!(answer.is_error, "{}", answer.result_json);
    assert!(
        answer
            .error_message
            .contains(&format!("session {plain} has no work queue")),
        "{}",
        answer.error_message
    );
    assert!(
        answer
            .error_message
            .contains(tddy_core::session_work_queue::NO_WORK_QUEUE_HINT),
        "{}",
        answer.error_message
    );
}

#[tokio::test]
async fn a_session_acts_on_its_own_stack_without_naming_it() {
    // Given an orchestrator whose changeset names its own stack, planning one PR, and another
    // stack of the same project
    let world = a_world().await;
    let a = &world.a;
    let (orchestrator, own_stack) = a.an_orchestrator_owning_a_stack_of(&["Store"]).await;
    let other_stack = a.holds_a_stack(&world.main, &["Elsewhere"]);

    // When it adds a planned PR and reads the status, naming no stack
    a.answer_to(
        &orchestrator,
        "work_item_add",
        json!({ "title": "Tools", "description": "the tools", "parents": ["n1"] }),
    )
    .await;
    let status = a
        .answer_to(&orchestrator, "work_queue_status", json!({}))
        .await;

    // Then both acted on its own stack, and the other stack is untouched
    assert_eq!(
        a.nodes_of(&own_stack),
        vec![node("n1", "Store", &[]), node("n2", "Tools", &["n1"])]
    );
    assert_eq!(
        status_nodes(&status),
        vec![pair("n1", "Store"), pair("n2", "Tools")]
    );
    assert_eq!(a.nodes_of(&other_stack), vec![node("n1", "Elsewhere", &[])]);
}

#[tokio::test]
async fn a_queue_held_by_another_host_that_addresses_main_is_read_through_forwarding_not_edited() {
    // Given a queue held by host-b, owned by a session there, whose item n1 links main (on host-a,
    // which owns no queue and holds no record of the one on host-b)
    let world = a_world().await;
    let owner_on_b = a_workspace_in(&world.b.daemon, THIS_PROJECT).await;
    let queue_id = world.b.holds_a_stack(&owner_on_b, &["Store"]);
    world
        .b
        .stacks()
        .update(THIS_PROJECT, &queue_id, |stack| {
            stack.nodes[0].session_id = Some(world.main.clone());
        })
        .unwrap();

    // When main reads its status and tries to plan on it, naming host-b
    let status = world
        .a
        .answer_to(
            &world.main,
            "work_queue_status",
            json!({ "queue_id": queue_id, "daemon_instance_id": HOST_B }),
        )
        .await;
    let planning = world
        .a
        .call_as(
            &world.main,
            "work_item_add",
            json!({
                "queue_id": queue_id,
                "daemon_instance_id": HOST_B,
                "title": "Tools",
                "description": "the tools",
            }),
        )
        .await
        .expect("a refusal on the holding host is the tool's answer");

    // Then host-b answered the status of the queue it holds, refused the write, and left it as it was
    assert_eq!(status_nodes(&status), vec![pair("n1", "Store")]);
    assert!(planning.is_error, "{}", planning.result_json);
    assert_eq!(world.b.nodes_of(&queue_id), vec![node("n1", "Store", &[])]);
    assert_eq!(
        world
            .peers
            .times_asked(HOST_B, EXEC_TOOL_SERVICE, "ExecuteTool"),
        2
    );
}

#[tokio::test]
async fn a_stack_node_session_is_offered_only_status_and_read_for_its_own_stack() {
    // Given an orchestrator whose node n1 is bound to a session it started, and another
    // stack of the project
    let world = a_world().await;
    let a = &world.a;
    let (orchestrator, own_stack) = a
        .an_orchestrator_owning_a_stack_of(&["Store", "Tools"])
        .await;
    let child = a.a_node_session_of(&orchestrator, &own_stack, "n1").await;
    let other_stack = a.holds_a_stack(&world.main, &["Elsewhere"]);

    // When the node's session reads its stack's status, reads another stack's, and plans a PR
    let status = a
        .answer_to(
            &child,
            "work_queue_status",
            json!({ "queue_id": own_stack }),
        )
        .await;
    let other = a
        .call_as(
            &child,
            "work_queue_status",
            json!({ "queue_id": other_stack }),
        )
        .await
        .expect("an addressing refusal is the tool's answer");
    let refusal = a
        .call_as(
            &child,
            "work_item_add",
            json!({ "title": "More", "description": "more" }),
        )
        .await
        .expect_err("a node may not plan its stack");

    // Then it is offered status and read only, reads its own stack, is refused the other stack by
    // name, and is refused the write as permission denied with nothing written
    assert_eq!(
        a.work_queue_tools_offered_to(&child).await,
        names(&["work_queue_status", "work_pr_read"])
    );
    assert_eq!(
        status_nodes(&status),
        vec![pair("n1", "Store"), pair("n2", "Tools")]
    );
    assert!(other.is_error, "{}", other.result_json);
    assert!(
        other.error_message.contains(&other_stack),
        "{}",
        other.error_message
    );
    assert_eq!(refusal.code(), Code::PermissionDenied, "{refusal:?}");
    assert_eq!(
        a.nodes_of(&own_stack),
        vec![node("n1", "Store", &[]), node("n2", "Tools", &[])]
    );
}

#[tokio::test]
async fn an_ordinary_session_is_offered_no_stack_tool_and_is_refused_one() {
    // Given an ordinary session of the project, and a stack of it
    let world = a_world().await;
    let a = &world.a;
    let ordinary = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let queue_id = a.holds_a_stack(&world.main, &["Store"]);

    // When it lists the project's stacks and plans a PR on that stack
    let listing = a
        .call_as(&ordinary, "work_queue_list", json!({}))
        .await
        .expect_err("an ordinary session is refused the stack tools");
    let planning = a
        .call_as(
            &ordinary,
            "work_item_add",
            json!({ "queue_id": queue_id, "title": "More", "description": "more" }),
        )
        .await
        .expect_err("an ordinary session is refused the stack tools");

    // Then it is offered no stack tool, both calls are refused as permission denied, and the stack
    // is untouched
    assert_eq!(
        a.work_queue_tools_offered_to(&ordinary).await,
        Vec::<String>::new()
    );
    assert_eq!(
        (listing.code(), planning.code()),
        (Code::PermissionDenied, Code::PermissionDenied),
        "{listing:?} / {planning:?}"
    );
    assert_eq!(a.nodes_of(&queue_id), vec![node("n1", "Store", &[])]);
}

#[tokio::test]
async fn work_item_update_with_sync_pr_publishes_with_the_project_accounts_token() {
    // Given a stack whose node n1 has an open pull request, #42, and main — a read-only session,
    // which holds no GitHub credential of its own
    let world = a_world().await;
    let a = &world.a;
    let queue_id = a.holds_a_stack(&world.main, &["Store"]);
    a.node_has_pull_request(&queue_id, "n1", 42);

    // When main retitles n1 and syncs its pull request
    let updated = a
        .answer_to(
            &world.main,
            "work_item_update",
            json!({
                "queue_id": queue_id,
                "node_id": "n1",
                "title": "Project stack store",
                "sync_pr": true,
            }),
        )
        .await;

    // Then the plan is retitled, and the pull request was retitled on the project's repository as
    // the project's account
    assert_eq!(updated["pr_synced"], json!(42));
    assert_eq!(
        a.nodes_of(&queue_id),
        vec![node("n1", "Project stack store", &[])]
    );
    assert_eq!(
        world.github.clients_built(),
        vec![(REPO.to_string(), PROJECTS_ACCOUNT_TOKEN.to_string())]
    );
    assert_eq!(
        world.github.title_body_edits(),
        vec![(42, Some("Project stack store".to_string()), None)]
    );
}

// ── #orchestrators 13/13: a node's reads in a jail and on another host ──────────────────────

#[tokio::test]
async fn a_stack_node_session_in_a_jail_is_listed_its_two_reads_as_the_daemon_lists_them() {
    // Given an orchestrator whose node n1 is bound to a session it started, which runs in a jail
    let world = a_world().await;
    let a = &world.a;
    let (orchestrator, own_stack) = a
        .an_orchestrator_owning_a_stack_of(&["Store", "Tools"])
        .await;
    let child = a.a_node_session_of(&orchestrator, &own_stack, "n1").await;

    // When the jail's agent asks which tools its session is offered
    let in_the_jail = a.work_queue_tools_listed_in_the_jail_of(&child).await;

    // Then the jail lists the node's two reads — exactly what the daemon lists it
    assert_eq!(in_the_jail, names(&["work_queue_status", "work_pr_read"]));
    assert_eq!(in_the_jail, a.work_queue_tools_offered_to(&child).await);
}

#[tokio::test]
async fn an_orchestrators_jail_is_listed_every_stack_tool_the_daemon_lists_it() {
    // Given an orchestrator owning a stack, running in a jail
    let world = a_world().await;
    let a = &world.a;
    let (orchestrator, _own_stack) = a.an_orchestrator_owning_a_stack_of(&["Store"]).await;

    // When the jail's agent asks which tools its session is offered
    let in_the_jail = a
        .work_queue_tools_listed_in_the_jail_of(&orchestrator)
        .await;

    // Then the jail lists all eleven stack tools — exactly what the daemon lists it
    assert_eq!(in_the_jail, names(&EVERY_STACK_TOOL));
    assert_eq!(
        in_the_jail,
        a.work_queue_tools_offered_to(&orchestrator).await
    );
}

#[tokio::test]
async fn a_stack_node_session_on_another_host_reads_its_stack_where_the_stack_is_held() {
    // Given an orchestrator on host-a owning a stack, whose node n1 is bound to a session on host-b
    let world = a_world().await;
    let (orchestrator, own_stack) = world
        .a
        .an_orchestrator_owning_a_stack_of(&["Store", "Tools"])
        .await;
    let child = a_node_session_held_by(&world.b, &world.a, &orchestrator, &own_stack, "n1").await;

    // When the node's agent reads its stack's status and plans a PR, each call addressed at host-a
    let status = world
        .b
        .call_addressed_at(
            HOST_A,
            &child,
            "work_queue_status",
            json!({ "queue_id": own_stack }),
        )
        .await
        .expect("host-a serves a node bound to the caller its stack's status");
    let planning = world
        .b
        .call_addressed_at(
            HOST_A,
            &child,
            "work_item_add",
            json!({ "queue_id": own_stack, "title": "More", "description": "more" }),
        )
        .await
        .expect("a refusal on the holding host is the tool's answer");

    // Then host-a answers the status of the stack it holds, refuses the write, and its stack is
    // untouched
    assert!(!status.is_error, "{}", status.error_message);
    let status: Value = serde_json::from_str(&status.result_json).expect("the answer is JSON");
    assert_eq!(
        status_nodes(&status),
        vec![pair("n1", "Store"), pair("n2", "Tools")]
    );
    assert!(planning.is_error, "{}", planning.result_json);
    assert_eq!(
        world.a.nodes_of(&own_stack),
        vec![node("n1", "Store", &[]), node("n2", "Tools", &[])]
    );
}

#[tokio::test]
async fn a_session_on_another_host_bound_to_no_node_there_is_refused_the_stack_reads() {
    // Given an orchestrator on host-a owning a stack, and an ordinary session on host-b that no
    // node of it is bound to
    let world = a_world().await;
    let (_orchestrator, own_stack) = world.a.an_orchestrator_owning_a_stack_of(&["Store"]).await;
    let stranger = a_workspace_in(&world.b.daemon, THIS_PROJECT).await;

    // When it reads that stack's status, addressed at host-a
    let refusal = world
        .b
        .call_addressed_at(
            HOST_A,
            &stranger,
            "work_queue_status",
            json!({ "queue_id": own_stack }),
        )
        .await
        .expect("a refusal on the holding host is the tool's answer");

    // Then host-a refuses it, saying the queue is not the session's
    assert!(refusal.is_error, "{}", refusal.result_json);
    assert!(
        refusal.error_message.contains("is not this session's"),
        "{}",
        refusal.error_message
    );
}
