//! Unit tests: `PrStackRpcHandler::add_planned_pr` — the stack guard (`require_a_stack`,
//! `#orchestrators` 12/12 B9) refusing a session that names no stack, whatever its recipe,
//! before its stack is touched. There is no first-write create on the session-addressed path.
//!
//! PRD: docs/ft/coder/pr-stacking.md § Manually adding a planned PR.
//! Changeset: docs/dev/1-WIP/pr-stack-manual-add-planned-pr.md.

use std::sync::Arc;

use super::PrStackRpcHandler;
use tddy_core::changeset::{read_changeset, Changeset};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_rpc::Request;
use tddy_service::proto::pr_stack::{AddPlannedPrRequest, PrStackService};
use tddy_session_lifecycle::cli_session_manager::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::PrStackServiceImpl;

fn make_unit_config() -> tddy_daemon_kernel::config::DaemonConfig {
    let yaml = "";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    std::fs::write(
        std::path::Path::new(&(path))
            .parent()
            .expect("a config directory")
            .join("users.yaml"),
        "- github_user: \"u\"\n  os_user: \"u\"\n",
    )
    .unwrap();
    tddy_daemon_kernel::config::DaemonConfig::load(&path).unwrap()
}

fn make_unit_stack_service(
    sessions_base: std::path::PathBuf,
) -> PrStackServiceImpl<PrStackRpcHandler> {
    PrStackServiceImpl::new(Arc::new(PrStackRpcHandler::from_host(
        &make_unit_connection(sessions_base),
    )))
}

fn make_unit_connection(sessions_base: std::path::PathBuf) -> DaemonSessionHost {
    let config = make_unit_config();
    let base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver = Arc::new(|token| {
        if token == "valid" {
            Some("u".to_string())
        } else {
            None
        }
    });
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        sessions_base,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
        tddy_agent_launch::host_coding_agents::no_coding_agents(),
    )
}

fn write_unit_changeset(session_dir: &std::path::Path, recipe: Option<&str>) {
    write_unit_changeset_named(session_dir, recipe, None);
}

fn write_unit_changeset_named(
    session_dir: &std::path::Path,
    recipe: Option<&str>,
    stack_id: Option<&str>,
) {
    let changeset = Changeset {
        recipe: recipe.map(str::to_string),
        stack_id: stack_id.map(str::to_string),
        ..Changeset::default()
    };
    tddy_changeset::stack_fixtures::write_changeset_with_stack(session_dir, changeset);
}

/// Seed a stack record in the host's store for `session_id`'s project and name it in the
/// session's changeset (the shape `require_a_stack` admits).
fn write_unit_changeset_naming_a_store_stack(
    sessions_base: &std::path::Path,
    session_dir: &std::path::Path,
    session_id: &str,
    recipe: Option<&str>,
) {
    let store =
        tddy_core::project_stack_store::ProjectStackStore::new(sessions_base.join("projects"));
    let record = store
        .create(
            "proj-test",
            session_id,
            tddy_core::changeset::Stack::default(),
        )
        .expect("seed a stack record");
    write_unit_changeset_named(session_dir, recipe, Some(&record.stack_id));
}

fn a_request(session_id: &str, title: &str) -> Request<AddPlannedPrRequest> {
    Request::direct(AddPlannedPrRequest {
        session_token: "valid".to_string(),
        session_id: session_id.to_string(),
        title: title.to_string(),
        description: String::new(),
        branch_suggestion: String::new(),
        parents: vec![],
        child_recipe: String::new(),
        stack: None,
    })
}

/// A session whose changeset `recipe` is `"tdd"` and which names no stack must be refused
/// before its stack is ever touched, naming `pr_stack_create`.
#[tokio::test]
async fn add_planned_pr_rejects_a_session_whose_recipe_is_not_pr_stack() {
    // Given — a plain "tdd" session, not a pr-stack orchestrator
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_stack_service(temp.path().to_path_buf());
    let session_dir = unified_session_dir_path(temp.path(), "tdd-session-1");
    write_unit_changeset(&session_dir, Some("tdd"));

    // When
    let result = service
        .add_planned_pr(a_request("tdd-session-1", "Add auth middleware"))
        .await;

    // Then
    let err = result.expect_err("a session naming no stack must be refused");
    assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
    assert!(
        err.message().contains("pr_stack_create"),
        "the refusal must name pr_stack_create, got: {}",
        err.message()
    );
    let loaded = read_changeset(&session_dir).unwrap();
    assert!(
        loaded.stack_id.is_none(),
        "the rejected session must not gain a stack"
    );
}

/// A session with no `recipe` set at all (legacy/never-tagged changeset) names no stack
/// either, and must be refused the same way.
#[tokio::test]
async fn add_planned_pr_rejects_a_session_with_no_recipe_set() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_stack_service(temp.path().to_path_buf());
    let session_dir = unified_session_dir_path(temp.path(), "no-recipe-session");
    write_unit_changeset(&session_dir, None);

    // When
    let result = service
        .add_planned_pr(a_request("no-recipe-session", "Add auth middleware"))
        .await;

    // Then
    let err = result.expect_err("a session naming no stack must be refused");
    assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
    assert!(
        err.message().contains("pr_stack_create"),
        "the refusal must name pr_stack_create, got: {}",
        err.message()
    );
}

/// A `pr-stack`-recipe session that names no stack is refused too: the recipe no longer
/// creates a stack on first write (the removed path, `#orchestrators` 12/12 B9).
#[tokio::test]
async fn add_planned_pr_rejects_a_pr_stack_recipe_session_that_names_no_stack() {
    // Given — a "pr-stack" recipe session whose changeset names no stack
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_stack_service(temp.path().to_path_buf());
    let session_dir = unified_session_dir_path(temp.path(), "pr-stack-no-stack-1");
    write_unit_changeset(&session_dir, Some("pr-stack"));

    // When
    let result = service
        .add_planned_pr(a_request("pr-stack-no-stack-1", "Add auth middleware"))
        .await;

    // Then
    let err = result.expect_err("a session naming no stack must be refused, whatever its recipe");
    assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
    assert!(
        err.message().contains("pr_stack_create"),
        "the refusal must name pr_stack_create, got: {}",
        err.message()
    );
}

/// A session that names a stack is accepted (whatever its recipe) and gains the new planned PR.
#[tokio::test]
async fn add_planned_pr_succeeds_for_a_pr_stack_orchestrator_session() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_stack_service(temp.path().to_path_buf());
    let session_dir = unified_session_dir_path(temp.path(), "pr-stack-session-1");
    write_unit_changeset_naming_a_store_stack(
        temp.path(),
        &session_dir,
        "pr-stack-session-1",
        Some("pr-stack"),
    );

    // When
    let result = service
        .add_planned_pr(a_request("pr-stack-session-1", "Add auth middleware"))
        .await;

    // Then
    let resp = result
        .expect("a pr-stack orchestrator session must be accepted")
        .into_inner();
    let parsed: serde_json::Value =
        serde_json::from_str(&resp.stack_plan_json).expect("stack_plan_json must be valid JSON");
    assert_eq!(parsed["nodes"][0]["node_id"], "n1");
    assert_eq!(parsed["nodes"][0]["title"], "Add auth middleware");
    assert_eq!(parsed["nodes"][0]["parents"], serde_json::json!([]));
    assert_eq!(
        tddy_changeset::stack_fixtures::stack_on_disk(&session_dir)
            .nodes
            .len(),
        1
    );
}

/// A session that names a stack is accepted whatever its recipe — including a legacy alias
/// ("orchestrate-pr-stack") and including no pr-stack recipe at all.
#[tokio::test]
async fn add_planned_pr_succeeds_for_a_legacy_orchestrate_pr_stack_alias_session() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_stack_service(temp.path().to_path_buf());
    let session_dir = unified_session_dir_path(temp.path(), "legacy-orchestrator-session");
    write_unit_changeset_naming_a_store_stack(
        temp.path(),
        &session_dir,
        "legacy-orchestrator-session",
        Some("orchestrate-pr-stack"),
    );

    // When
    let result = service
        .add_planned_pr(a_request(
            "legacy-orchestrator-session",
            "Add auth middleware",
        ))
        .await;

    // Then
    assert!(
        result.is_ok(),
        "a legacy orchestrate-pr-stack alias session must still be accepted"
    );
}
