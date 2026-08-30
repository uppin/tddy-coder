//! Acceptance: `LinkStackNode` — the write half of `ResolveStackBase`, so a child spawned on one
//! host can record itself on a planned node that lives on another.
//!
//! `link_stack_node_to_spawned_branch` writes the orchestrator's `changeset.yaml` on the **spawning**
//! daemon's own sessions tree. Spawn a planned PR's session on host B under an orchestrator on host A
//! and there is no such session there: the lookup returns `None`, the write is silently skipped, and
//! the node keeps neither a branch nor a child forever — which wedges every descendant, because
//! `Stack::base_ref_for_spawn` gates on a parent owning a branch. The code has carried a
//! `TODO(cross-host-pr-stack)` naming exactly this.
//!
//! The link therefore becomes an RPC, routed to the daemon that owns the orchestrator before the
//! caller is authenticated — the same shape, and for the same reason, as `ResolveStackBase`: the
//! parent's stack lives in *its* session's `changeset.yaml`, so only the daemon holding that session
//! can write it (D35).
//!
//! Two things this file does **not** re-prove: that `rpc_served_by_peer` reaches a real peer over
//! LiveKit (`multi_host_acceptance::start_session_remote_daemon_instance_id_routes_to_peer` covers
//! the mechanism), and that `link_stack_node_to_child_session` writes the fields
//! (`stack_child_linking_acceptance::link_stack_node_sets_session_id_and_branch`). What is proven
//! here is the RPC's own contract: what it records, what it refuses, and that a request naming
//! another daemon is routed rather than answered from the wrong disk.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D34–D36).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::changeset::{Changeset, Stack, StackNode};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::connection_service::{
    ConnectionServiceImpl, SessionUserResolver, SessionsBaseResolver,
};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, LinkStackNodeRequest,
};

const ORCHESTRATOR: &str = "orchestrator-1";
const CHILD: &str = "dddddddd-0000-4000-8000-000000000004";
const TOKEN: &str = "valid-session-token";
const BRANCH: &str = "feature/attach-docs/attach-store";

fn a_service(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(
        &path,
        "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n",
    )
    .unwrap();
    let config = tddy_daemon::config::DaemonConfig::load(&path).unwrap();
    let base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver =
        Arc::new(|token| (token == TOKEN).then(|| "u".to_string()));
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        sessions_base,
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

fn write_changeset(sessions_base: &Path, session_id: &str, changeset: &Changeset) {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    tddy_core::write_changeset(&dir, changeset).unwrap();
}

fn a_planned_node(node_id: &str) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: format!("Planned {node_id}"),
        description: String::new(),
        branch_suggestion: Some(BRANCH.to_string()),
        branch: None,
        session_id: None,
        parents: vec![],
        pr_status: None,
        child_state: None,
        internal_status: None,
        display_order: None,
    }
}

/// A pr-stack orchestrator holding one unlinked planned node.
fn an_orchestrator_with_an_unlinked_node(sessions_base: &Path) {
    write_changeset(
        sessions_base,
        ORCHESTRATOR,
        &Changeset {
            recipe: Some("pr-stack".to_string()),
            stack: Some(Stack {
                version: 1,
                nodes: vec![a_planned_node("n1")],
            }),
            ..Changeset::default()
        },
    );
}

fn a_link_request(node_id: &str) -> LinkStackNodeRequest {
    LinkStackNodeRequest {
        session_token: TOKEN.to_string(),
        daemon_instance_id: String::new(),
        orchestrator_session_id: ORCHESTRATOR.to_string(),
        node_id: node_id.to_string(),
        child_session_id: CHILD.to_string(),
        branch: BRANCH.to_string(),
    }
}

/// The stack the orchestrator holds on disk after a call.
fn stack_on_disk(sessions_base: &Path) -> Stack {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(ORCHESTRATOR);
    tddy_core::read_changeset(&dir)
        .expect("the orchestrator's changeset must still be readable")
        .stack
        .expect("the orchestrator must still hold its stack")
}

#[tokio::test]
async fn records_the_child_session_on_the_named_planned_node() {
    // Given — the node the child's own host could not write, because it holds no such session
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    an_orchestrator_with_an_unlinked_node(&sessions_base);
    let service = a_service(sessions_base.clone());

    // When
    service
        .link_stack_node(Request::new(a_link_request("n1")))
        .await
        .expect("LinkStackNode must succeed on the daemon that owns the orchestrator");

    // Then
    let node = stack_on_disk(&sessions_base)
        .node("n1")
        .cloned()
        .expect("n1 must still exist");
    assert_eq!(node.session_id.as_deref(), Some(CHILD));
}

#[tokio::test]
async fn records_the_branch_the_child_created_on_the_named_planned_node() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    an_orchestrator_with_an_unlinked_node(&sessions_base);
    let service = a_service(sessions_base.clone());

    // When
    service
        .link_stack_node(Request::new(a_link_request("n1")))
        .await
        .expect("LinkStackNode must succeed");

    // Then — the branch is what unwedges the node's descendants, so it is the load-bearing half
    let node = stack_on_disk(&sessions_base)
        .node("n1")
        .cloned()
        .expect("n1 must still exist");
    assert_eq!(node.branch.as_deref(), Some(BRANCH));
}

#[tokio::test]
async fn answers_with_the_plan_as_it_stands_after_the_link() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    an_orchestrator_with_an_unlinked_node(&sessions_base);
    let service = a_service(sessions_base);

    // When
    let plan_json = service
        .link_stack_node(Request::new(a_link_request("n1")))
        .await
        .expect("LinkStackNode must succeed")
        .into_inner()
        .stack_plan_json;

    // Then — a caller that renders the plan does not re-read it, exactly as the other stack
    // mutations already answer
    let stack: Stack =
        serde_json::from_str(&plan_json).expect("the response must carry a serialized Stack");
    assert_eq!(
        stack
            .node("n1")
            .and_then(|n| n.branch.as_deref())
            .unwrap_or_default(),
        BRANCH
    );
}

#[tokio::test]
async fn refuses_a_node_id_the_plan_does_not_hold() {
    // Given — the failure the local write already has: it finds no node and returns success
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    an_orchestrator_with_an_unlinked_node(&sessions_base);
    let service = a_service(sessions_base.clone());

    // When
    let err = service
        .link_stack_node(Request::new(a_link_request("no-such-node")))
        .await
        .expect_err("a link that lands nowhere must be reported, never reported as done");

    // Then
    assert_eq!(
        err.code,
        Code::NotFound,
        "got {:?}: {}",
        err.code,
        err.message
    );
    assert!(
        stack_on_disk(&sessions_base)
            .node("n1")
            .expect("n1 must still exist")
            .branch
            .is_none(),
        "a refused link must write nothing"
    );
}

#[tokio::test]
async fn refuses_a_session_that_is_not_a_pr_stack_orchestrator() {
    // Given — an ordinary session, which holds no stack to write into
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    write_changeset(
        &sessions_base,
        ORCHESTRATOR,
        &Changeset {
            recipe: Some("tdd".to_string()),
            ..Changeset::default()
        },
    );
    let service = a_service(sessions_base);

    // When
    let err = service
        .link_stack_node(Request::new(a_link_request("n1")))
        .await
        .expect_err("LinkStackNode must refuse a session that carries no stack");

    // Then — `require_pr_stack_orchestrator` has one answer for a session whose recipe is not
    // `pr-stack`, and accepting a second code would let a refusal for an unrelated reason pass here
    assert_eq!(
        err.code,
        Code::FailedPrecondition,
        "got {:?}: {}",
        err.code,
        err.message
    );
}

#[tokio::test]
async fn requires_a_node_id_rather_than_deriving_one_from_the_branch() {
    // Given — the branch is the operator's to edit in the create dialog before confirming, so
    // deriving the node from it silently unlinks a renamed branch from the row that started it (D34)
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    an_orchestrator_with_an_unlinked_node(&sessions_base);
    let service = a_service(sessions_base.clone());

    // When
    let err = service
        .link_stack_node(Request::new(a_link_request("")))
        .await
        .expect_err("LinkStackNode must refuse an unnamed node");

    // Then — the node whose `branch_suggestion` matches is deliberately not guessed at
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "got {:?}: {}",
        err.code,
        err.message
    );
    assert!(
        stack_on_disk(&sessions_base)
            .node("n1")
            .expect("n1 must still exist")
            .session_id
            .is_none(),
        "a refused link must write nothing"
    );
}

#[tokio::test]
async fn routes_a_request_naming_another_daemon_instead_of_answering_from_local_disk() {
    // Given — this daemon holds the orchestrator, but the request names a different host: answering
    // it here would write the wrong tree, which is the bug this RPC exists to remove
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    an_orchestrator_with_an_unlinked_node(&sessions_base);
    let service = a_service(sessions_base.clone());

    // When — the named peer is in no common room, so routing fails rather than falling back
    let mut req = a_link_request("n1");
    req.daemon_instance_id = "fabricated-peer-not-in-discovery".to_string();
    let err = service
        .link_stack_node(Request::new(req))
        .await
        .expect_err("a request addressed to an unreachable peer must not be answered locally");

    // Then — `rpc_served_by_peer` refuses a daemon it shares no common room with as a bad request,
    // and the message names it: without that, a refusal raised for any other reason would pass this
    // test while the call was never routed at all
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "got {:?}: {}",
        err.code,
        err.message
    );
    assert!(
        err.message.contains("fabricated-peer-not-in-discovery"),
        "the refusal must come from the routing leg and name the peer it addressed; got: {}",
        err.message
    );
    assert!(
        stack_on_disk(&sessions_base)
            .node("n1")
            .expect("n1 must still exist")
            .session_id
            .is_none(),
        "a routed request must never write this daemon's own tree"
    );
}

#[tokio::test]
async fn refuses_a_link_that_names_no_branch() {
    // Given — a link carrying no branch sets the one field that matters to nothing:
    // `link_stack_node_to_child_session` leaves `node.branch` untouched, so the node still refuses
    // every descendant while the RPC answers `Ok` (D35)
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    an_orchestrator_with_an_unlinked_node(&sessions_base);
    let service = a_service(sessions_base.clone());

    // When
    let mut req = a_link_request("n1");
    req.branch = "   ".to_string();
    let err = service
        .link_stack_node(Request::new(req))
        .await
        .expect_err("a link that would record no branch must be refused, not reported as done");

    // Then
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "got {:?}: {}",
        err.code,
        err.message
    );
    assert!(
        stack_on_disk(&sessions_base)
            .node("n1")
            .expect("n1 must still exist")
            .session_id
            .is_none(),
        "a refused link must write nothing"
    );
}

#[tokio::test]
async fn refuses_a_link_that_names_no_child_session() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    an_orchestrator_with_an_unlinked_node(&sessions_base);
    let service = a_service(sessions_base.clone());

    // When
    let mut req = a_link_request("n1");
    req.child_session_id = String::new();
    let err = service
        .link_stack_node(Request::new(req))
        .await
        .expect_err("a link names the session that materialized the node");

    // Then
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "got {:?}: {}",
        err.code,
        err.message
    );
    assert!(
        stack_on_disk(&sessions_base)
            .node("n1")
            .expect("n1 must still exist")
            .branch
            .is_none(),
        "a refused link must write nothing"
    );
}

#[tokio::test]
async fn refuses_a_caller_whose_session_token_is_not_valid() {
    // Given — the call is routed before authentication, so the daemon that serves it is the one
    // that verifies the token; an unverified caller must not reach another user's sessions tree
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    an_orchestrator_with_an_unlinked_node(&sessions_base);
    let service = a_service(sessions_base.clone());

    // When
    let mut req = a_link_request("n1");
    req.session_token = "expired-session-token".to_string();
    let err = service
        .link_stack_node(Request::new(req))
        .await
        .expect_err("an unauthenticated caller must be refused");

    // Then
    assert_eq!(
        err.code,
        Code::Unauthenticated,
        "got {:?}: {}",
        err.code,
        err.message
    );
    assert!(
        stack_on_disk(&sessions_base)
            .node("n1")
            .expect("n1 must still exist")
            .branch
            .is_none(),
        "a refused link must write nothing"
    );
}

#[tokio::test]
async fn refuses_an_orchestrator_session_id_that_walks_out_of_the_sessions_tree() {
    // Given — `orchestrator_session_id` is joined into a filesystem path, so a caller that can put
    // `..` in it can address another user's session directory
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    an_orchestrator_with_an_unlinked_node(&sessions_base);
    let service = a_service(sessions_base.clone());

    // When
    let mut req = a_link_request("n1");
    req.orchestrator_session_id = "../other-user".to_string();
    let err = service
        .link_stack_node(Request::new(req))
        .await
        .expect_err("a traversing session id must never be resolved to a path");

    // Then
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "got {:?}: {}",
        err.code,
        err.message
    );
}
