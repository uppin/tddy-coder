//! Reproduction: transitional workflow states (`Planning`, `RedTesting`, `Evaluating`, …) are
//! emitted to the UI via `WorkflowEvent::StateChange`, but `changeset.yaml` must also advance
//! immediately when each goal **starts** so resume (`next_goal_for_state` / `run_workflow`) and
//! tooling see the same phase as the user. Otherwise disk can stay on the previous completed
//! state (e.g. `GreenComplete` while evaluation runs — fixed for evaluate — or `Planned` while
//! red runs) and restart picks the wrong goal.

mod common;

use std::path::Path;

use tddy_core::changeset::{
    read_changeset, write_changeset, Changeset, ChangesetState, SessionEntry,
};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;

use common::{
    write_changeset_with_state, write_evaluation_report_to_session_dir, write_refactoring_plan,
};
use tddy_workflow_recipes::TddWorkflowHooks;

fn write_session_dir_red_ready(session_dir: &Path) {
    std::fs::create_dir_all(session_dir).expect("create plan dir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan\n").expect("PRD");
    std::fs::write(session_dir.join("acceptance-tests.md"), "# AT\n- t1\n")
        .expect("acceptance-tests.md");
}

fn write_session_dir_green_ready(session_dir: &Path) {
    write_session_dir_red_ready(session_dir);
    std::fs::write(
        session_dir.join("progress.md"),
        "# Progress\n## Tests\n- t1 done\n",
    )
    .expect("progress.md");
    let cs = Changeset {
        name: Some("feature".to_string()),
        sessions: vec![
            SessionEntry {
                id: "plan-s".to_string(),
                agent: "claude".to_string(),
                tag: "plan".to_string(),
                created_at: "2026-03-07T10:00:00Z".to_string(),
                system_prompt_file: None,
            },
            SessionEntry {
                id: "impl-s".to_string(),
                agent: "claude".to_string(),
                tag: "impl".to_string(),
                created_at: "2026-03-07T10:00:00Z".to_string(),
                system_prompt_file: None,
            },
        ],
        state: ChangesetState {
            current: WorkflowState::new("RedTestsReady"),
            session_id: Some("impl-s".to_string()),
            updated_at: "2026-03-07T10:00:00Z".to_string(),
            ..Changeset::default().state
        },
        branch_suggestion: Some("feature/test".to_string()),
        worktree_suggestion: Some("feature-test".to_string()),
        ..Changeset::default()
    };
    write_changeset(session_dir, &cs).expect("write changeset");
}

/// Starting `plan` must persist `Planning` so the manifest matches the active goal.
#[tokio::test]
async fn before_task_persists_planning_when_plan_starts() {
    let session_dir =
        std::env::temp_dir().join(format!("tddy-cs-trans-plan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("mkdir");
    write_changeset_with_state(&session_dir, "Init", "sess-plan");

    let hooks = TddWorkflowHooks::new(common::tdd_recipe(), common::tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("backend_name", "claude".to_string());
    ctx.set_sync("feature_input", "My feature");

    hooks.before_task("plan", &ctx).expect("before_task plan");

    let cs = read_changeset(&session_dir).expect("read changeset");
    assert_eq!(
        cs.state.current,
        WorkflowState::new("Planning"),
        "changeset must record Planning as soon as the plan goal starts"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Starting `acceptance-tests` must persist `AcceptanceTesting`.
#[tokio::test]
async fn before_task_persists_acceptance_testing_when_acceptance_tests_start() {
    let output_dir =
        std::env::temp_dir().join(format!("tddy-cs-trans-at-out-{}", std::process::id()));
    let session_dir = output_dir.join("feat-slug");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&session_dir).expect("mkdir");
    write_changeset_with_state(&session_dir, "Planned", "sess-at");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Summary\nx").expect("PRD");

    let hooks = TddWorkflowHooks::new(common::tdd_recipe(), common::tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("output_dir", output_dir.clone());
    ctx.set_sync("backend_name", "stub".to_string());

    hooks
        .before_task("acceptance-tests", &ctx)
        .expect("before_task acceptance-tests");

    let cs = read_changeset(&session_dir).expect("read changeset");
    assert_eq!(
        cs.state.current,
        WorkflowState::new("AcceptanceTesting"),
        "changeset must record AcceptanceTesting as soon as acceptance-tests starts"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Starting `red` must persist `RedTesting`.
#[tokio::test]
async fn before_task_persists_red_testing_when_red_starts() {
    let session_dir =
        std::env::temp_dir().join(format!("tddy-cs-trans-red-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    write_session_dir_red_ready(&session_dir);
    write_changeset_with_state(&session_dir, "AcceptanceTestsReady", "sess-red");

    let hooks = TddWorkflowHooks::new(common::tdd_recipe(), common::tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("backend_name", "claude".to_string());

    hooks.before_task("red", &ctx).expect("before_task red");

    let cs = read_changeset(&session_dir).expect("read changeset");
    assert_eq!(
        cs.state.current,
        WorkflowState::new("RedTesting"),
        "changeset must record RedTesting as soon as red starts"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Starting `green` must persist `GreenImplementing`.
#[tokio::test]
async fn before_task_persists_green_implementing_when_green_starts() {
    let session_dir =
        std::env::temp_dir().join(format!("tddy-cs-trans-green-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    write_session_dir_green_ready(&session_dir);

    let hooks = TddWorkflowHooks::new(common::tdd_recipe(), common::tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("backend_name", "claude".to_string());
    ctx.set_sync("run_optional_step_x", false);

    hooks.before_task("green", &ctx).expect("before_task green");

    let cs = read_changeset(&session_dir).expect("read changeset");
    assert_eq!(
        cs.state.current,
        WorkflowState::new("GreenImplementing"),
        "changeset must record GreenImplementing as soon as green starts"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Starting `demo` must persist `DemoRunning`.
#[tokio::test]
async fn before_task_persists_demo_running_when_demo_starts() {
    let session_dir =
        std::env::temp_dir().join(format!("tddy-cs-trans-demo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("mkdir");
    write_changeset_with_state(&session_dir, "GreenComplete", "sess-demo");
    std::fs::write(session_dir.join("demo-plan.md"), "# Demo\nsteps\n").expect("demo-plan");

    let hooks = TddWorkflowHooks::new(common::tdd_recipe(), common::tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("backend_name", "claude".to_string());

    hooks.before_task("demo", &ctx).expect("before_task demo");

    let cs = read_changeset(&session_dir).expect("read changeset");
    assert_eq!(
        cs.state.current,
        WorkflowState::new("DemoRunning"),
        "changeset must record DemoRunning as soon as demo starts"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Starting `validate` must persist `Validating`.
#[tokio::test]
async fn before_task_persists_validating_when_validate_starts() {
    let session_dir =
        std::env::temp_dir().join(format!("tddy-cs-trans-val-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("mkdir");
    write_changeset_with_state(&session_dir, "Evaluated", "sess-val");
    write_evaluation_report_to_session_dir(&session_dir);

    let hooks = TddWorkflowHooks::new(common::tdd_recipe(), common::tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("backend_name", "claude".to_string());

    hooks
        .before_task("validate", &ctx)
        .expect("before_task validate");

    let cs = read_changeset(&session_dir).expect("read changeset");
    assert_eq!(
        cs.state.current,
        WorkflowState::new("Validating"),
        "changeset must record Validating as soon as validate starts"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Starting `refactor` must persist `Refactoring`.
#[tokio::test]
async fn before_task_persists_refactoring_when_refactor_starts() {
    let session_dir =
        std::env::temp_dir().join(format!("tddy-cs-trans-ref-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("mkdir");
    write_changeset_with_state(&session_dir, "ValidateComplete", "sess-ref");
    write_refactoring_plan(&session_dir);

    let hooks = TddWorkflowHooks::new(common::tdd_recipe(), common::tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("backend_name", "claude".to_string());

    hooks
        .before_task("refactor", &ctx)
        .expect("before_task refactor");

    let cs = read_changeset(&session_dir).expect("read changeset");
    assert_eq!(
        cs.state.current,
        WorkflowState::new("Refactoring"),
        "changeset must record Refactoring as soon as refactor starts"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Starting `update-docs` must persist `UpdatingDocs`.
#[tokio::test]
async fn before_task_persists_updating_docs_when_update_docs_starts() {
    let session_dir =
        std::env::temp_dir().join(format!("tddy-cs-trans-docs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("mkdir");
    write_changeset_with_state(&session_dir, "RefactorComplete", "sess-docs");
    std::fs::write(session_dir.join("PRD.md"), "# P\n").expect("PRD");

    let hooks = TddWorkflowHooks::new(common::tdd_recipe(), common::tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("backend_name", "claude".to_string());

    hooks
        .before_task("update-docs", &ctx)
        .expect("before_task update-docs");

    let cs = read_changeset(&session_dir).expect("read changeset");
    assert_eq!(
        cs.state.current,
        WorkflowState::new("UpdatingDocs"),
        "changeset must record UpdatingDocs as soon as update-docs starts"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}
