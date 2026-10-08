//! The engine-driven stack tasks ask their session's host for a GitHub token only when an action
//! that reaches GitHub actually runs — never when the recipe is built, and never for a stack that
//! has nothing for GitHub to answer.
//!
//! The token is the project account's, asked per call over the session's own toolcall socket
//! (`TDDY_SOCKET`); a refusal is the host's own words, surfaced at the action that needed the token.
//! The process environment is not a source: `GITHUB_TOKEN` changes nothing.
//!
//! Changeset: docs/dev/1-WIP/2026-09-19-keyring-github-identity.md

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serial_test::serial;
use tddy_core::changeset::{write_changeset_atomic, Changeset, Stack, StackNode};
use tddy_core::toolcall::{GithubCredentialHandler, ToolcallRpcService};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_core::workflow::task::Task;
use tddy_workflow_recipes::orchestrate_pr_stack::{
    AssessTask, MergeTask, OrchestratePrStackRecipe, RepointTask,
};

const REFUSAL: &str = "this project has no github account assigned; assign one to the project";

/// A session host that refuses every `github-token` in [`REFUSAL`]'s words and counts the asks.
struct RefusingHost {
    asked: Arc<AtomicUsize>,
}

#[async_trait]
impl GithubCredentialHandler for RefusingHost {
    async fn github_token(&self) -> Result<String, String> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Err(REFUSAL.to_string())
    }
}

/// A session host listening on its own socket, on a runtime of its own — as it is in production, a
/// different process — so the asker blocking its thread cannot stall it.
struct SessionHost {
    _dir: tempfile::TempDir,
    asked: Arc<AtomicUsize>,
}

impl SessionHost {
    fn requests(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

/// Run a refusing session host and point `TDDY_SOCKET` at it, as a managed session does.
fn a_session_host_refusing_every_request() -> SessionHost {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let socket: PathBuf = dir.path().join("host.sock");
    let asked = Arc::new(AtomicUsize::new(0));
    let handler: Arc<dyn GithubCredentialHandler> = Arc::new(RefusingHost {
        asked: Arc::clone(&asked),
    });
    let bound = std::os::unix::net::UnixListener::bind(&socket).expect("bind the host socket");
    bound.set_nonblocking(true).expect("non-blocking socket");
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the host's runtime");
        runtime.block_on(async move {
            let listener = tokio::net::UnixListener::from_std(bound).expect("adopt the socket");
            let (tx, _rx) = std::sync::mpsc::sync_channel(1);
            while let Ok((stream, _)) = listener.accept().await {
                let service = ToolcallRpcService::new(
                    tx.clone(),
                    Arc::new(None),
                    Arc::new(None),
                    Arc::new(std::env::temp_dir()),
                )
                .with_github_credential_handler(Some(Arc::clone(&handler)));
                let (reader, writer) = stream.into_split();
                let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                    reader,
                    writer,
                    service,
                    tddy_rpc::RequestTransport::UnixSocket,
                );
                tokio::spawn(endpoint.run());
            }
        });
    });
    std::env::set_var("TDDY_SOCKET", &socket);
    SessionHost { _dir: dir, asked }
}

fn a_node(node_id: &str, branch: Option<&str>) -> StackNode {
    StackNode {
        node_id: node_id.into(),
        title: node_id.into(),
        description: String::new(),
        branch_suggestion: None,
        branch: branch.map(str::to_string),
        session_id: None,
        parents: vec![],
        pr_status: None,
        child_state: None,
        internal_status: None,
        display_order: None,
    }
}

fn an_orchestrator_with(nodes: Vec<StackNode>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    write_changeset_atomic(
        dir.path(),
        &Changeset {
            stack: Some(Stack { version: 1, nodes }),
            ..Default::default()
        },
    )
    .expect("write the changeset");
    dir
}

fn assess_context(session_dir: &Path) -> Context {
    let context = Context::new();
    context.set_sync("session_dir", session_dir.to_path_buf());
    context
}

fn merge_context(session_dir: &Path) -> Context {
    let context = assess_context(session_dir);
    context.set_sync("merge_node_id", "n1".to_string());
    context.set_sync("merge_pr_number", 7_u64);
    context
}

fn repoint_context(session_dir: &Path, dependents: &[&str]) -> Context {
    let context = assess_context(session_dir);
    context.set_sync("merge_node_id", "n1".to_string());
    context.set_sync(
        "merge_dependents",
        dependents.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
    );
    context
}

/// What a task's run reports, as the text of its refusal when it failed.
fn failure_of(
    result: Result<impl std::fmt::Debug, Box<dyn std::error::Error + Send + Sync>>,
) -> String {
    result
        .expect_err("the task should have been refused")
        .to_string()
}

#[test]
#[serial]
fn building_the_recipe_asks_the_host_for_nothing() {
    // Given a session host that would refuse any request
    let host = a_session_host_refusing_every_request();

    // When the recipe's graph is built
    let _graph =
        OrchestratePrStackRecipe.build_graph(Arc::new(tddy_core::backend::StubBackend::new()));

    // Then the host was never asked
    assert_eq!(host.requests(), 0);
}

#[tokio::test]
#[serial]
async fn assessing_a_stack_whose_nodes_own_no_branch_asks_the_host_for_nothing() {
    // Given a stack with nothing for GitHub to answer, and a host that would refuse
    let host = a_session_host_refusing_every_request();
    let orchestrator = an_orchestrator_with(vec![a_node("n1", None), a_node("n2", None)]);

    // When it is assessed
    let assessed = AssessTask::new()
        .run(assess_context(orchestrator.path()))
        .await;

    // Then it completes, and the host was never asked
    assert_eq!(
        assessed.expect("assess completes").response,
        "assess → spawn"
    );
    assert_eq!(host.requests(), 0);
}

#[tokio::test]
#[serial]
async fn assessing_a_stack_with_a_branch_asks_once_and_surfaces_the_hosts_refusal() {
    // Given a stack whose node owns a branch, and a host whose project assigns no account
    let host = a_session_host_refusing_every_request();
    let orchestrator = an_orchestrator_with(vec![a_node("n1", Some("feature/auth-store"))]);

    // When it is assessed
    let refusal = failure_of(
        AssessTask::new()
            .run(assess_context(orchestrator.path()))
            .await,
    );

    // Then the host was asked once, and its words reach the caller
    assert_eq!(host.requests(), 1);
    assert!(refusal.contains(REFUSAL), "got: {refusal}");
}

#[tokio::test]
#[serial]
async fn repointing_dependents_that_own_no_branch_asks_the_host_for_nothing() {
    // Given a merged node whose dependent owns no branch yet, and a host that would refuse
    let host = a_session_host_refusing_every_request();
    let orchestrator =
        an_orchestrator_with(vec![a_node("n1", Some("feature/a")), a_node("n2", None)]);

    // When the dependents are repointed
    let repointed = RepointTask::new()
        .run(repoint_context(orchestrator.path(), &["n2"]))
        .await;

    // Then the plan-only repoint completes, and the host was never asked
    assert_eq!(
        repointed.expect("repoint completes").response,
        "repoint complete — returning to assess"
    );
    assert_eq!(host.requests(), 0);
}

#[tokio::test]
#[serial]
async fn repointing_a_dependent_that_owns_a_branch_asks_once_and_surfaces_the_hosts_refusal() {
    // Given a merged node whose dependent owns a branch, and a host whose project assigns no account
    let host = a_session_host_refusing_every_request();
    let orchestrator = an_orchestrator_with(vec![
        a_node("n1", Some("feature/a")),
        a_node("n2", Some("feature/b")),
    ]);

    // When the dependents are repointed
    let refusal = failure_of(
        RepointTask::new()
            .run(repoint_context(orchestrator.path(), &["n2"]))
            .await,
    );

    // Then the host was asked once, and its words reach the caller
    assert_eq!(host.requests(), 1);
    assert!(refusal.contains(REFUSAL), "got: {refusal}");
}

#[tokio::test]
#[serial]
async fn merging_asks_once_and_surfaces_the_hosts_refusal_before_anything_is_journaled() {
    // Given a node ready to merge, and a host whose project assigns no account
    let host = a_session_host_refusing_every_request();
    let orchestrator = an_orchestrator_with(vec![a_node("n1", Some("feature/a"))]);

    // When the merge runs
    let refusal = failure_of(
        MergeTask::new()
            .run(merge_context(orchestrator.path()))
            .await,
    );

    // Then the host was asked once, its words reach the caller, and no in-flight merge was recorded
    assert_eq!(host.requests(), 1);
    assert!(refusal.contains(REFUSAL), "got: {refusal}");
    assert!(!orchestrator.path().join(".workflow/stack-op.json").exists());
}

#[tokio::test]
#[serial]
async fn a_github_token_in_the_environment_rescues_no_refused_merge() {
    // Given GITHUB_TOKEN exported where the agent runs, and a host whose project assigns no account
    std::env::set_var("GITHUB_TOKEN", "ghp_from_the_environment");
    std::env::set_var("GH_TOKEN", "ghp_from_the_environment");
    let host = a_session_host_refusing_every_request();
    let orchestrator = an_orchestrator_with(vec![a_node("n1", Some("feature/a"))]);

    // When the merge runs
    let refusal = failure_of(
        MergeTask::new()
            .run(merge_context(orchestrator.path()))
            .await,
    );

    // Then the environment rescued nothing: the host was asked and refused
    assert_eq!(host.requests(), 1);
    assert!(refusal.contains(REFUSAL), "got: {refusal}");
}

#[tokio::test]
#[serial]
async fn a_github_token_in_the_environment_rescues_no_refused_assess() {
    // Given GITHUB_TOKEN exported where the agent runs, and a host whose project assigns no account
    std::env::set_var("GITHUB_TOKEN", "ghp_from_the_environment");
    std::env::set_var("GH_TOKEN", "ghp_from_the_environment");
    let host = a_session_host_refusing_every_request();
    let orchestrator = an_orchestrator_with(vec![a_node("n1", Some("feature/auth-store"))]);

    // When it is assessed
    let refusal = failure_of(
        AssessTask::new()
            .run(assess_context(orchestrator.path()))
            .await,
    );

    // Then the environment rescued nothing: the host was asked and refused
    assert_eq!(host.requests(), 1);
    assert!(refusal.contains(REFUSAL), "got: {refusal}");
}
