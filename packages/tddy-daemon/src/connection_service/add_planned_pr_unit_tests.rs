//! Unit tests: `ConnectionServiceImpl::add_planned_pr` — the recipe guard rejecting a
//! non-"pr-stack" session before its `Changeset.stack` is touched.
//!
//! PRD: docs/ft/coder/pr-stacking.md § Manually adding a planned PR.
//! Changeset: docs/dev/1-WIP/pr-stack-manual-add-planned-pr.md.

use super::*;
use tddy_core::changeset::{read_changeset, write_changeset};

fn make_unit_config() -> crate::config::DaemonConfig {
    let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    crate::config::DaemonConfig::load(&path).unwrap()
}

fn make_unit_service(sessions_base: std::path::PathBuf) -> ConnectionServiceImpl {
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

fn write_unit_changeset(session_dir: &std::path::Path, recipe: Option<&str>) {
    std::fs::create_dir_all(session_dir).unwrap();
    let changeset = Changeset {
        recipe: recipe.map(str::to_string),
        ..Changeset::default()
    };
    write_changeset(session_dir, &changeset).unwrap();
}

fn a_request(session_id: &str, title: &str) -> Request<AddPlannedPrRequest> {
    Request::new(AddPlannedPrRequest {
        session_token: "valid".to_string(),
        session_id: session_id.to_string(),
        title: title.to_string(),
        description: String::new(),
        branch_suggestion: String::new(),
        parents: vec![],
        child_recipe: String::new(),
    })
}

/// A session whose changeset `recipe` is `"tdd"` (not a pr-stack orchestrator) must be
/// rejected before `Changeset.stack` is ever touched.
#[tokio::test]
async fn add_planned_pr_rejects_a_session_whose_recipe_is_not_pr_stack() {
    // Given — a plain "tdd" session, not a pr-stack orchestrator
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_service(temp.path().to_path_buf());
    let session_dir = unified_session_dir_path(temp.path(), "tdd-session-1");
    write_unit_changeset(&session_dir, Some("tdd"));

    // When
    let result = service
        .add_planned_pr(a_request("tdd-session-1", "Add auth middleware"))
        .await;

    // Then
    let err = result.expect_err("a non-pr-stack session must be rejected");
    assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
    let loaded = read_changeset(&session_dir).unwrap();
    assert!(
        loaded.stack.is_none(),
        "the rejected session's Changeset.stack must remain untouched"
    );
}

/// A session with no `recipe` set at all (legacy/never-tagged changeset) is not a pr-stack
/// orchestrator either, and must be rejected the same way.
#[tokio::test]
async fn add_planned_pr_rejects_a_session_with_no_recipe_set() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_service(temp.path().to_path_buf());
    let session_dir = unified_session_dir_path(temp.path(), "no-recipe-session");
    write_unit_changeset(&session_dir, None);

    // When
    let result = service
        .add_planned_pr(a_request("no-recipe-session", "Add auth middleware"))
        .await;

    // Then
    let err = result.expect_err("a session with no recipe must be rejected");
    assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
}

/// A genuine "pr-stack" orchestrator session is accepted and gains the new planned PR.
#[tokio::test]
async fn add_planned_pr_succeeds_for_a_pr_stack_orchestrator_session() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_service(temp.path().to_path_buf());
    let session_dir = unified_session_dir_path(temp.path(), "pr-stack-session-1");
    write_unit_changeset(&session_dir, Some("pr-stack"));

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
    let loaded = read_changeset(&session_dir).unwrap();
    assert_eq!(loaded.stack.unwrap().nodes.len(), 1);
}

/// A legacy alias recipe name ("orchestrate-pr-stack") resolves to the same canonical
/// "pr-stack" recipe and must also be accepted — this guard must not regress old sessions.
#[tokio::test]
async fn add_planned_pr_succeeds_for_a_legacy_orchestrate_pr_stack_alias_session() {
    // Given
    let temp = tempfile::tempdir().unwrap();
    let service = make_unit_service(temp.path().to_path_buf());
    let session_dir = unified_session_dir_path(temp.path(), "legacy-orchestrator-session");
    write_unit_changeset(&session_dir, Some("orchestrate-pr-stack"));

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
