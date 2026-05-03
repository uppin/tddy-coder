//! Acceptance tests for bugfix **interview** phase (PRD Testing Plan).
//! Red until `BugfixRecipe` graph/metadata, `bugfix::interview` prompts, and analyze handoff land.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::backend::{GoalHints, PermissionHint, StubBackend};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_core::GoalId;
use tddy_workflow_recipes::bugfix::interview as bugfix_interview;
use tddy_workflow_recipes::bugfix::{BugfixRecipe, BugfixWorkflowHooks};

const BUGFIX_INTERVIEW_HANDOFF_RELATIVE: &str = ".workflow/bugfix_interview_handoff.txt";

/// PRD: **interview → analyze → reproduce → end**; `next_task_id(interview) == analyze`.
#[test]
fn bugfix_graph_orders_interview_before_analyze() {
    let backend = Arc::new(StubBackend::new());
    let recipe = BugfixRecipe;
    let graph = recipe.build_graph(backend);

    let ctx = Context::new();
    assert!(
        graph.get_task("interview").is_some(),
        "bugfix graph must register an `interview` BackendInvokeTask (shared goal id with TDD where safe)"
    );
    assert_eq!(
        graph.next_task_id("interview", &ctx),
        Some("analyze".to_string()),
        "edge interview → analyze"
    );
    assert_eq!(
        graph.next_task_id("analyze", &ctx),
        Some("reproduce".to_string()),
        "edge analyze → reproduce"
    );
    assert_eq!(
        graph.next_task_id("reproduce", &ctx),
        Some("end".to_string()),
        "edge reproduce → end"
    );

    let r: Arc<dyn WorkflowRecipe> = Arc::new(recipe);
    assert_eq!(
        r.start_goal(),
        GoalId::new("interview"),
        "bugfix entry goal must be interview (before analyze)"
    );
    assert_eq!(
        r.plan_refinement_goal(),
        GoalId::new("analyze"),
        "plan refinement after primary doc review must target analyze, not interview (override trait default)"
    );
    let goal_ids = r.goal_ids();
    let ids: Vec<&str> = goal_ids.iter().map(|g| g.as_str()).collect();
    assert!(
        ids.iter()
            .position(|g| *g == "interview")
            .unwrap_or(usize::MAX)
            < ids
                .iter()
                .position(|g| *g == "analyze")
                .unwrap_or(usize::MAX),
        "goal_ids must list interview before analyze: {:?}",
        ids
    );

    // Red extension: align initial persisted marker with TDD interview resume semantics (Green implements).
    assert_eq!(
        recipe.initial_state().as_str(),
        "Interview",
        "bugfix initial WorkflowState should mirror TddRecipe::initial_state for interview-first resume"
    );
}

/// PRD: `goal_requires_tddy_tools_submit(interview) == false`; interview is a first-class elicitation goal with hints.
#[test]
fn bugfix_interview_goal_opted_out_of_structured_submit() {
    let r: Arc<dyn WorkflowRecipe> = Arc::new(BugfixRecipe);
    let gid = GoalId::new("interview");
    assert!(
        !r.goal_requires_tddy_tools_submit(&gid),
        "interview must complete without structured tddy-tools submit (like TDD interview)"
    );
    let hints = r.goal_hints(&gid);
    assert!(
        hints.is_some(),
        "BugfixRecipe must expose GoalHints for interview"
    );
    let GoalHints {
        agent_output,
        permission,
        agent_cli_plan_mode,
        ..
    } = hints.expect("checked");
    assert!(
        agent_output,
        "interview should surface agent output for elicitation"
    );
    assert_eq!(
        permission,
        PermissionHint::ReadOnly,
        "interview elicitation matches read-only grill-style phase"
    );
    assert!(
        !agent_cli_plan_mode,
        "interview must not require vendor plan-mode CLI flags"
    );

    // Red: full prompt contract must mention tool schema wiring (PRD + changeset-workflow schema).
    let p = bugfix_interview::system_prompt();
    assert!(
        p.contains("tool_schema_id"),
        "bugfix interview system prompt must document tool_schema_id for demo/workflow routing; got len {}",
        p.len()
    );
}

/// PRD: combined interview system + user prompt contract (`tddy-tools ask`, persistence, demo fields).
#[test]
fn bugfix_interview_prompt_requires_demo_and_persistence_contract() {
    let hooks = BugfixWorkflowHooks::new(None);
    let ctx = Context::new();
    hooks
        .before_task("interview", &ctx)
        .expect("before_task interview must succeed");

    let system = ctx
        .get_sync::<String>("system_prompt")
        .expect("interview before_task must set system_prompt on context");
    let user = ctx.get_sync::<String>("prompt").unwrap_or_default();
    let combined = format!("{}\n{}", system, user);
    for needle in [
        "tddy-tools ask",
        "persist-changeset-workflow",
        "changeset.yaml",
        "run_optional_step_x",
        "demo_options",
    ] {
        assert!(
            combined.contains(needle),
            "bugfix interview prompt contract must include `{needle}`; combined len={}",
            combined.len()
        );
    }
}

/// PRD: relay `.workflow/bugfix_interview_handoff.txt` merged into analyze context in `before_task(analyze)`.
#[test]
fn bugfix_interview_handoff_visible_to_analyze_context() {
    let tmp = std::env::temp_dir().join(format!(
        "tddy-bugfix-interview-handoff-accept-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join(".workflow")).unwrap();
    const MARKER: &str = "BUGFIX_INTERVIEW_HANDOFF_ACCEPTANCE_XYZZY";
    fs::write(tmp.join(BUGFIX_INTERVIEW_HANDOFF_RELATIVE), MARKER).unwrap();

    let hooks = BugfixWorkflowHooks::new(None);
    let ctx = Context::new();
    ctx.set_sync("session_dir", tmp.clone());
    ctx.set_sync(
        "output_dir",
        PathBuf::from("/tmp/bugfix-handoff-output-placeholder"),
    );
    ctx.set_sync("feature_input", "repro: flaky test when …");
    ctx.set_sync("prompt", "original user bug text before merge");

    hooks
        .before_task("analyze", &ctx)
        .expect("before_task analyze must succeed");

    let prompt = ctx.get_sync::<String>("prompt").unwrap_or_default();
    assert!(
        prompt.contains(MARKER),
        "analyze context prompt must include relayed interview clarification; got prompt len {}",
        prompt.len()
    );
}
