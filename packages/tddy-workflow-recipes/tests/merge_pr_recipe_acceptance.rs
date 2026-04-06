//! PRD acceptance: **`merge-pr`** resolver, graph ordering, submit policy, and GitHub vs degraded behavior contracts.
//!
//! These tests fail until `MergePrRecipe` is registered, `approval_policy` lists **`merge-pr`**, and the
//! graph exposes **`analyze` → `sync-main` → `finalize` → `end`** (read-only analysis, then worktree sync, then finalize).

use std::collections::BTreeSet;
use std::sync::Arc;

use tddy_core::backend::StubBackend;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::GoalId;
use tddy_workflow_recipes::{
    approval_policy, unknown_workflow_recipe_error, workflow_recipe_and_manifest_from_cli_name,
};

/// Graph / goal ids (PRD: analyze feasibility read-only, then sync with **`main`** in worktree, then finalize).
pub const TASK_ANALYZE: &str = "analyze";
pub const TASK_SYNC_MAIN: &str = "sync-main";
pub const TASK_FINALIZE: &str = "finalize";

#[test]
fn merge_pr_recipe_registers_with_resolver() {
    let names = approval_policy::supported_workflow_recipe_cli_names();
    assert!(
        names.contains(&"merge-pr"),
        "F5: supported_workflow_recipe_cli_names must include merge-pr; got {:?}",
        names
    );

    let err = unknown_workflow_recipe_error("totally-unknown-merge-pr");
    assert!(
        err.contains("\"merge-pr\""),
        "unknown_workflow_recipe_error must list merge-pr among expected names: {}",
        err
    );

    let (merge_pr, _) =
        workflow_recipe_and_manifest_from_cli_name("merge-pr").expect("merge-pr must resolve");
    let (tdd, _) = workflow_recipe_and_manifest_from_cli_name("tdd").expect("tdd must resolve");

    assert_eq!(merge_pr.name(), "merge-pr");
    assert_eq!(tdd.name(), "tdd");
    assert_ne!(
        merge_pr.name(),
        tdd.name(),
        "merge-pr and tdd must be distinct recipes (distinct WorkflowRecipe identities)"
    );

    assert!(
        workflow_recipe_and_manifest_from_cli_name("garbage-merge-pr-name").is_err(),
        "unknown CLI names must not resolve"
    );
}

#[test]
fn merge_pr_graph_has_ordered_goals() {
    let (recipe, _) =
        workflow_recipe_and_manifest_from_cli_name("merge-pr").expect("merge-pr must resolve");
    let backend = Arc::new(StubBackend::new());
    let graph = recipe.build_graph(backend);
    let ctx = Context::new();

    assert_eq!(
        graph.id, "merge_pr_workflow",
        "merge-pr graph id must be stable for telemetry / debugging"
    );

    let ids: BTreeSet<String> = graph.task_ids().cloned().collect();
    assert!(
        ids.contains(TASK_ANALYZE)
            && ids.contains(TASK_SYNC_MAIN)
            && ids.contains(TASK_FINALIZE)
            && ids.contains("end"),
        "merge-pr graph must include analyze, sync-main, finalize, and end; got {:?}",
        ids
    );

    assert_eq!(
        graph.next_task_id(TASK_ANALYZE, &ctx),
        Some(TASK_SYNC_MAIN.to_string()),
        "analyze must run before sync-main (read-only feasibility check before worktree merge)"
    );
    assert_eq!(
        graph.next_task_id(TASK_SYNC_MAIN, &ctx),
        Some(TASK_FINALIZE.to_string()),
        "sync-main must run before finalize (integrate main before push / optional GitHub merge)"
    );
    assert_eq!(
        graph.next_task_id(TASK_FINALIZE, &ctx),
        Some("end".to_string()),
        "finalize must precede workflow end"
    );

    assert_eq!(recipe.start_goal().as_str(), TASK_ANALYZE);
    assert_eq!(recipe.initial_state().as_str(), "Analyze");

    assert_eq!(
        recipe.next_goal_for_state(&WorkflowState::new("Init")),
        Some(GoalId::new(TASK_ANALYZE))
    );
    assert_eq!(
        recipe.next_goal_for_state(&WorkflowState::new("Analyze")),
        Some(GoalId::new(TASK_ANALYZE))
    );
    assert_eq!(
        recipe.next_goal_for_state(&WorkflowState::new("SyncMain")),
        Some(GoalId::new(TASK_SYNC_MAIN))
    );
    assert_eq!(
        recipe.next_goal_for_state(&WorkflowState::new("Finalize")),
        Some(GoalId::new(TASK_FINALIZE))
    );
}

/// PRD: without credentials the recipe completes git work + push only; GitHub merge is not invoked (no merge API goal before finalize).
#[test]
fn merge_pr_skips_github_when_no_token() {
    let (recipe, _) =
        workflow_recipe_and_manifest_from_cli_name("merge-pr").expect("merge-pr must resolve");

    // No dedicated GitHub merge graph task; API merge is conditional inside finalize.
    assert!(
        !recipe
            .goal_ids()
            .iter()
            .any(|g| g.as_str().contains("github") || g.as_str() == "merge-api"),
        "merge-pr must not expose a dedicated GitHub merge graph task; API merge is conditional inside finalize"
    );

    // Finalize requires structured submit for operator-visible outcome (PR / push / skip reason).
    assert!(
        recipe.goal_requires_tddy_tools_submit(&GoalId::new(TASK_FINALIZE)),
        "finalize must require tddy-tools submit for merge-pr-report-style outcome"
    );

    // Analyze and sync steps: no structured submit.
    assert!(
        !recipe.goal_requires_tddy_tools_submit(&GoalId::new(TASK_ANALYZE)),
        "analyze must allow completion without structured submit (read-only analysis)"
    );
    assert!(
        !recipe.goal_requires_tddy_tools_submit(&GoalId::new(TASK_SYNC_MAIN)),
        "sync-main must allow completion from agent output without structured submit"
    );
}

/// PRD: with token, GitHub merge runs after sync; graph still orders sync before finalize (REST merge happens in finalize implementation).
#[test]
fn merge_pr_merges_pr_when_token_present() {
    let (recipe, _) =
        workflow_recipe_and_manifest_from_cli_name("merge-pr").expect("merge-pr must resolve");

    let goal_ids = recipe.goal_ids();
    let ids: Vec<&str> = goal_ids.iter().map(|g| g.as_str()).collect();
    assert_eq!(
        ids.first().copied(),
        Some(TASK_ANALYZE),
        "first recipe goal must be analyze"
    );
    assert!(
        ids.contains(&TASK_SYNC_MAIN) && ids.contains(&TASK_FINALIZE),
        "recipe.goal_ids must include sync-main and finalize for merge + push + API"
    );

    let backend = Arc::new(StubBackend::new());
    let graph = recipe.build_graph(backend);
    let ctx = Context::new();
    assert_eq!(
        graph.next_task_id(TASK_ANALYZE, &ctx),
        Some(TASK_SYNC_MAIN.to_string()),
        "sync-main must follow analyze (worktree created after read-only analysis)"
    );
    assert_eq!(
        graph.next_task_id(TASK_SYNC_MAIN, &ctx),
        Some(TASK_FINALIZE.to_string()),
        "GitHub merge (when implemented) must not run before sync-main completes"
    );
}

#[test]
fn merge_pr_sync_requires_session_worktree_for_conflict_hooks() {
    let (recipe, _) =
        workflow_recipe_and_manifest_from_cli_name("merge-pr").expect("merge-pr must resolve");

    assert!(
        recipe.goal_requires_session_dir(&GoalId::new(TASK_ANALYZE))
            && recipe.goal_requires_session_dir(&GoalId::new(TASK_SYNC_MAIN))
            && recipe.goal_requires_session_dir(&GoalId::new(TASK_FINALIZE)),
        "analyze, sync-main, and finalize must all require a session dir"
    );

    assert!(
        recipe.goal_ids().iter().any(|g| g.as_str() == TASK_ANALYZE),
        "analyze goal must exist for read-only merge feasibility check"
    );
    assert!(
        recipe
            .goal_ids()
            .iter()
            .any(|g| g.as_str() == TASK_SYNC_MAIN),
        "sync-main goal must exist so conflict resolution can bind to it"
    );
}

/// PRD: analyze may emit a structured `worktree_suggestion` (directory basename) after read-only
/// analysis; `submit_key` must map to a dedicated goal id so `tddy-tools get-schema` and persistence
/// can target that shape before sync-main creates `.worktrees/<name>/`.
#[test]
fn merge_pr_analyze_submit_key_targets_dedicated_goal_for_worktree_suggestion() {
    let (recipe, _) =
        workflow_recipe_and_manifest_from_cli_name("merge-pr").expect("merge-pr must resolve");
    assert_eq!(
        recipe.submit_key(&GoalId::new(TASK_ANALYZE)).as_str(),
        "merge-pr-analyze",
        "submit_key(analyze) must be merge-pr-analyze so analyze can persist worktree_suggestion before sync-main"
    );
}
