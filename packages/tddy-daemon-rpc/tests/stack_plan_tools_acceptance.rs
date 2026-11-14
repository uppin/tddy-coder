//! Acceptance: the PR-stack plan and read tools are daemon exec tools addressed by `stack_id`
//! (`#orchestrators` 7/11). Main creates a titled stack, plans it and reads its status from its
//! read-only session; an orchestrator's tools default to its own stack (the one its changeset
//! names); a stack another host holds is
//! read and edited through forwarding; a node of a stack is offered status and read for its own stack
//! alone; an ordinary session is offered none and refused; and a synced edit reaches GitHub as the
//! project's account.
//!
//! Stacks are the project's records (`ProjectStackStore` under each host's `projects/`), read back
//! here through the real store. GitHub is a recording stand-in attached to the daemon
//! (`RpcHandlers::attach_stack_github`); the project is a Local gh one, so its account's token is
//! what the host's `gh` stub answers. Cross-host calls go through `support`'s in-memory peer
//! forwarder, never LiveKit.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-plan-tools.md
//! Changeset: docs/dev/1-WIP/2026-11-10-stack-plan-tools.md

mod support;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use support::{
    a_host_as_current_user_with_gh_at, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B,
    THIS_PROJECT,
};
use tddy_core::changeset::{read_changeset, write_changeset, GithubPrStatus, Stack, StackNode};
use tddy_core::project_stack_store::{ProjectStackStore, StackRecord};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::WorkflowError;
use tddy_daemon_rpc::exec_tool::stack_tools::{StackGithub, StackGithubClients};
use tddy_github::pr_api::{
    CheckRun, GithubPrApi, GithubPrInsightApi, PrDetail, PrFile, PrIssueComment, PrLookupOutcome,
    PrRef, PrReview, PrReviewComment, PrSearchHit, PrSearchQuery,
};
use tddy_projects::project_storage::{read_projects, write_projects};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::gh_cli::a_gh_cli;
use tddy_tool_engine::stack_tools::is_stack_tool;
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

impl StackGithubClients for RecordingGithub {
    fn client_for(&self, repo: &str, token: &str) -> Box<dyn StackGithub> {
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
    fn stacks(&self) -> ProjectStackStore {
        ProjectStackStore::new(self.sessions.path().join("projects"))
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

    /// A stack of the project held by this host, planned by `created_by`, with `titles` as its
    /// root nodes `n1`, `n2`, ….
    fn holds_a_stack(&self, created_by: &str, titles: &[&str]) -> String {
        self.stacks()
            .create(THIS_PROJECT, created_by, a_stack_of(titles))
            .unwrap()
            .stack_id
    }

    /// An orchestrator of the project on this host whose own stack — the one its changeset names —
    /// has `titles` as its nodes. No recipe is involved. Answers the session and its stack.
    async fn an_orchestrator_owning_a_stack_of(&self, titles: &[&str]) -> (String, String) {
        let session_id = a_workspace_in(&self.daemon, THIS_PROJECT).await;
        let stack_id = self.holds_a_stack(&session_id, titles);
        let dir = self.session_dir(&session_id);
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.session_role = Some("orchestrator".to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
        let mut changeset = read_changeset(&dir).unwrap_or_default();
        changeset.stack_id = Some(stack_id.clone());
        write_changeset(&dir, &changeset).unwrap();
        (session_id, stack_id)
    }

    /// A session of the project on this host started by `orchestrator` for `node_id` of its stack:
    /// the node is bound to it, and its changeset names `orchestrator`.
    async fn a_node_session_of(&self, orchestrator: &str, stack_id: &str, node_id: &str) -> String {
        let session_id = a_workspace_in(&self.daemon, THIS_PROJECT).await;
        let dir = self.session_dir(&session_id);
        let mut changeset = read_changeset(&dir).unwrap_or_default();
        changeset.orchestrator_session_id = Some(orchestrator.to_string());
        write_changeset(&dir, &changeset).unwrap();
        self.stacks()
            .update(THIS_PROJECT, stack_id, |stack| {
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

    /// `node_id` of `stack_id` records its open pull request, number `number`.
    fn node_has_pull_request(&self, stack_id: &str, node_id: &str, number: u64) {
        self.stacks()
            .update(THIS_PROJECT, stack_id, |stack| {
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

    /// The nodes of `stack_id` as this host's store holds them, as `(node_id, title, parents)`.
    fn nodes_of(&self, stack_id: &str) -> Vec<(String, String, Vec<String>)> {
        nodes_of(&self.stacks().read(THIS_PROJECT, stack_id).unwrap())
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
    async fn stack_tools_offered_to(&self, session_id: &str) -> Vec<String> {
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

fn a_stack_of(titles: &[&str]) -> Stack {
    Stack {
        version: 1,
        nodes: titles
            .iter()
            .enumerate()
            .map(|(i, title)| StackNode {
                node_id: format!("n{}", i + 1),
                title: title.to_string(),
                ..StackNode::default()
            })
            .collect(),
    }
}

fn nodes_of(record: &StackRecord) -> Vec<(String, String, Vec<String>)> {
    record
        .stack
        .nodes
        .iter()
        .map(|n| (n.node_id.clone(), n.title.clone(), n.parents.clone()))
        .collect()
}

/// The `(node_id, title)` of every node a `pr_stack_status` answer lists.
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

const EVERY_STACK_TOOL: [&str; 11] = [
    "pr_stack_list",
    "pr_stack_create",
    "pr_stack_status",
    "pr_read",
    "pr_search",
    "pr_comments",
    "pr_add_planned",
    "pr_update_planned",
    "pr_delete_planned",
    "pr_set_status",
    "pr_adopt",
];

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn main_creates_a_stack_adds_planned_prs_and_reads_its_status_from_a_read_only_session() {
    // Given main, a read-only session of the project
    let world = a_world().await;
    let a = &world.a;

    // When it creates a titled stack, plans two PRs on it, reads its status and lists the stacks
    let created = a
        .answer_to(
            &world.main,
            "pr_stack_create",
            json!({ "title": "Orchestrator sessions" }),
        )
        .await;
    let stack_id = created["stack_id"]
        .as_str()
        .expect("a stack_id")
        .to_string();
    let first = a
        .answer_to(
            &world.main,
            "pr_add_planned",
            json!({ "stack_id": stack_id, "title": "Store", "description": "the store" }),
        )
        .await;
    let first_id = first["node_id"].as_str().expect("a node_id").to_string();
    a.answer_to(
        &world.main,
        "pr_add_planned",
        json!({
            "stack_id": stack_id,
            "title": "Tools",
            "description": "the tools",
            "parents": [first_id],
        }),
    )
    .await;
    let status = a
        .answer_to(
            &world.main,
            "pr_stack_status",
            json!({ "stack_id": stack_id }),
        )
        .await;
    let listed = a.answer_to(&world.main, "pr_stack_list", json!({})).await;

    // Then main is offered every stack tool, the project's store holds the titled stack main
    // created with both planned PRs, the status lists them, and the list shows the title
    assert_eq!(
        a.stack_tools_offered_to(&world.main).await,
        names(&EVERY_STACK_TOOL)
    );
    let record = a.stacks().read(THIS_PROJECT, &stack_id).unwrap();
    assert_eq!(
        (
            record.created_by_session_id.as_str(),
            record.title.as_deref()
        ),
        (world.main.as_str(), Some("Orchestrator sessions"))
    );
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
            .expect("a list answer lists stacks")
            .iter()
            .map(|s| (s["stack_id"].clone(), s["title"].clone()))
            .collect::<Vec<_>>(),
        vec![(json!(stack_id), json!("Orchestrator sessions"))]
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
        "pr_add_planned",
        json!({ "title": "Tools", "description": "the tools", "parents": ["n1"] }),
    )
    .await;
    let status = a
        .answer_to(&orchestrator, "pr_stack_status", json!({}))
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
async fn a_stack_owned_by_another_host_is_read_and_edited_through_forwarding() {
    // Given a stack of the project held by host-b, and main on host-a
    let world = a_world().await;
    let stack_id = world.b.holds_a_stack(&world.main, &["Store"]);

    // When main lists host-b's stacks, plans a PR on that stack and reads its status, naming host-b
    let listed = world
        .a
        .answer_to(
            &world.main,
            "pr_stack_list",
            json!({ "daemon_instance_id": HOST_B }),
        )
        .await;
    world
        .a
        .answer_to(
            &world.main,
            "pr_add_planned",
            json!({
                "stack_id": stack_id,
                "daemon_instance_id": HOST_B,
                "title": "Tools",
                "description": "the tools",
                "parents": ["n1"],
            }),
        )
        .await;
    let status = world
        .a
        .answer_to(
            &world.main,
            "pr_stack_status",
            json!({ "stack_id": stack_id, "daemon_instance_id": HOST_B }),
        )
        .await;

    // Then each call was forwarded to host-b, which listed and edited its own record — host-a holds
    // no stack
    let listed_stacks: Vec<(String, String)> = listed["stacks"]
        .as_array()
        .expect("a list answer lists stacks")
        .iter()
        .map(|s| {
            (
                s["stack_id"].as_str().unwrap().to_string(),
                s["daemon_instance_id"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(listed_stacks, vec![(stack_id.clone(), HOST_B.to_string())]);
    assert_eq!(
        world.b.nodes_of(&stack_id),
        vec![node("n1", "Store", &[]), node("n2", "Tools", &["n1"])]
    );
    assert_eq!(
        status_nodes(&status),
        vec![pair("n1", "Store"), pair("n2", "Tools")]
    );
    assert_eq!(world.a.stacks().list(THIS_PROJECT).unwrap(), Vec::new());
    assert_eq!(
        world
            .peers
            .times_asked(HOST_B, EXEC_TOOL_SERVICE, "ExecuteTool"),
        3
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
    let status = a.answer_to(&child, "pr_stack_status", json!({})).await;
    let other = a
        .call_as(
            &child,
            "pr_stack_status",
            json!({ "stack_id": other_stack }),
        )
        .await
        .expect("an addressing refusal is the tool's answer");
    let refusal = a
        .call_as(
            &child,
            "pr_add_planned",
            json!({ "title": "More", "description": "more" }),
        )
        .await
        .expect_err("a node may not plan its stack");

    // Then it is offered status and read only, reads its own stack, is refused the other stack by
    // name, and is refused the write as permission denied with nothing written
    assert_eq!(
        a.stack_tools_offered_to(&child).await,
        names(&["pr_stack_status", "pr_read"])
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
    let stack_id = a.holds_a_stack(&world.main, &["Store"]);

    // When it lists the project's stacks and plans a PR on that stack
    let listing = a
        .call_as(&ordinary, "pr_stack_list", json!({}))
        .await
        .expect_err("an ordinary session is refused the stack tools");
    let planning = a
        .call_as(
            &ordinary,
            "pr_add_planned",
            json!({ "stack_id": stack_id, "title": "More", "description": "more" }),
        )
        .await
        .expect_err("an ordinary session is refused the stack tools");

    // Then it is offered no stack tool, both calls are refused as permission denied, and the stack
    // is untouched
    assert_eq!(
        a.stack_tools_offered_to(&ordinary).await,
        Vec::<String>::new()
    );
    assert_eq!(
        (listing.code(), planning.code()),
        (Code::PermissionDenied, Code::PermissionDenied),
        "{listing:?} / {planning:?}"
    );
    assert_eq!(a.nodes_of(&stack_id), vec![node("n1", "Store", &[])]);
}

#[tokio::test]
async fn pr_update_planned_with_sync_pr_publishes_with_the_project_accounts_token() {
    // Given a stack whose node n1 has an open pull request, #42, and main — a read-only session,
    // which holds no GitHub credential of its own
    let world = a_world().await;
    let a = &world.a;
    let stack_id = a.holds_a_stack(&world.main, &["Store"]);
    a.node_has_pull_request(&stack_id, "n1", 42);

    // When main retitles n1 and syncs its pull request
    let updated = a
        .answer_to(
            &world.main,
            "pr_update_planned",
            json!({
                "stack_id": stack_id,
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
        a.nodes_of(&stack_id),
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
