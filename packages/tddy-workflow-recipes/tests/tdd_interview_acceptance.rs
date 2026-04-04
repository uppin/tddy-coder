//! Acceptance tests for TDD **interview** step (PRD Testing Plan).
//! Expected Red state until `interview` is wired in graph, `TddRecipe` metadata, and hooks handoff.

use std::sync::Arc;

use tddy_core::backend::MockBackend;
use tddy_core::changeset::{write_changeset, Changeset};
use tddy_core::output::create_session_dir_in;
use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_core::workflow::runner::FlowRunner;
use tddy_core::workflow::session::{FileSessionStorage, Session, SessionStorage};
use tddy_core::{GoalId, PermissionHint};
use tddy_workflow_recipes::tdd::graph::build_tdd_workflow_graph;
use tddy_workflow_recipes::TddRecipe;

#[test]
fn tdd_recipe_start_goal_is_interview() {
    let recipe = TddRecipe;
    assert_eq!(
        recipe.initial_state().as_str(),
        "Interview",
        "TDD workflow should expose a distinct Interview initial state (session dir / changeset parity)"
    );
    assert_eq!(
        recipe.start_goal(),
        GoalId::new("interview"),
        "TDD workflow entry goal must be interview (before plan)"
    );
    let ids: Vec<String> = recipe
        .goal_ids()
        .into_iter()
        .map(|g| g.to_string())
        .collect();
    assert_eq!(
        ids.first().map(String::as_str),
        Some("interview"),
        "goal_ids must list interview first"
    );
    assert!(
        ids.contains(&"interview".to_string()) && ids.contains(&"plan".to_string()),
        "goal_ids must include both interview and plan"
    );
}

#[test]
fn tdd_interview_goal_hints_and_submit_policy() {
    let recipe = TddRecipe;
    let gid = GoalId::new("interview");
    let hints = recipe.goal_hints(&gid);
    assert!(
        hints.is_some(),
        "TddRecipe must expose GoalHints for interview (elicitation step)"
    );
    let h = hints.expect("checked");
    assert!(
        h.agent_output,
        "interview should surface agent output (grill-me-style elicitation)"
    );
    assert_eq!(
        h.permission,
        PermissionHint::ReadOnly,
        "grill-me grill uses read-only style elicitation before structured plan output"
    );
    assert!(
        !recipe.goal_requires_tddy_tools_submit(&gid),
        "interview must not require tddy-tools submit (grill-me parity for elicitation)"
    );
}

#[tokio::test]
async fn tdd_interview_handoff_populates_plan_context() {
    const HANDOFF_MARKER: &str = "TDD_INTERVIEW_HANDOFF_CANNED_ANSWER_XYZZY";

    let backend = Arc::new(MockBackend::new());
    backend.push_ok_without_submit("interview turn complete");
    let plan_json = r##"{"goal":"plan","prd":"# PRD\n\n## TODO\n\n- [ ] Task","todo_items":[{"id":"1","title":"Task","done":false}]}"##;
    backend.push_ok(plan_json);

    let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TddRecipe);
    let graph = Arc::new(build_tdd_workflow_graph(backend.clone(), recipe.clone()));

    assert!(
        graph.get_task("interview").is_some(),
        "graph must include interview before handoff can be verified"
    );

    let hooks = recipe.create_hooks(None);
    let dir = std::env::temp_dir().join(format!("tdd-interview-handoff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let storage = Arc::new(FileSessionStorage::new(dir.clone()));

    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).expect("repo root");
    let session_base = dir.join("session_base");
    let session_dir = create_session_dir_in(&session_base).expect("session dir");
    let init_cs = Changeset {
        initial_prompt: Some("feature blurb SKIP_QUESTIONS".to_string()),
        ..Changeset::default()
    };
    let _ = write_changeset(&session_dir, &init_cs);

    let session = Session::new_from_task(
        "handoff1".to_string(),
        "tdd_workflow".to_string(),
        "interview".to_string(),
    );
    session
        .context
        .set_sync("feature_input", "feature blurb SKIP_QUESTIONS".to_string());
    session.context.set_sync("output_dir", repo);
    session.context.set_sync("session_dir", session_dir.clone());
    session
        .context
        .set_sync("answers", HANDOFF_MARKER.to_string());

    storage.save(&session).await.expect("save session");

    let runner = FlowRunner::new_with_hooks(graph, storage.clone(), Some(hooks));

    let r1 = runner.run("handoff1").await.expect("run interview");
    assert_eq!(
        r1.current_task_id,
        Some("plan".to_string()),
        "after interview the workflow must advance to plan"
    );

    let _r2 = runner.run("handoff1").await.expect("run plan");

    let invocations = backend.invocations();
    let plan_invoke = invocations
        .iter()
        .find(|r| r.goal_id.as_str() == "plan")
        .expect("plan invoke must be recorded after interview");
    assert!(
        plan_invoke.prompt.contains(HANDOFF_MARKER),
        "plan prompt must include interview handoff content; got:\n{}",
        plan_invoke.prompt
    );

    let _ = std::fs::remove_dir_all(&dir);
}
