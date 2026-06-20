//! Granular regression tests for bugfix **interview** (prompts, relay merge, host recovery gate).

use std::fs;

use tddy_core::workflow::context::Context;
use tddy_workflow_recipes::bugfix::interview::{
    apply_bugfix_interview_handoff_to_analyze_context, system_prompt,
    BUGFIX_INTERVIEW_HANDOFF_RELATIVE,
};

#[test]
fn bugfix_interview_system_prompt_requires_socket_backed_ask_contract() {
    // When
    let p = system_prompt();

    // Then
    assert!(p.contains("tddy-tools ask"), "must require tddy-tools ask; got: {p:?}");
    assert!(p.contains("TDDY_SOCKET"), "must mention TDDY_SOCKET; got: {p:?}");
}

#[test]
fn bugfix_apply_handoff_merges_relay_into_prompt() {
    // Given
    let tmp = std::env::temp_dir().join(format!(
        "tddy-bugfix-handoff-granular-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join(".workflow")).unwrap();
    const MARKER: &str = "BUGFIX_RELAY_GRANULAR_MARKER";
    fs::write(tmp.join(BUGFIX_INTERVIEW_HANDOFF_RELATIVE), MARKER).unwrap();
    let ctx = Context::new();
    ctx.set_sync("prompt", "BASE_PROMPT");

    // When
    apply_bugfix_interview_handoff_to_analyze_context(tmp.as_path(), &ctx)
        .expect("apply handoff must succeed");

    // Then
    let prompt = ctx.get_sync::<String>("prompt").unwrap_or_default();
    assert!(
        prompt.contains(MARKER),
        "relay merge must include handoff marker in analyze-visible prompt; got {prompt:?}"
    );
}

#[test]
fn bugfix_host_clarification_gate_surfaces_recovery_for_interview_goal() {
    use std::sync::Arc;

    use tddy_core::workflow::context::Context;
    use tddy_core::workflow::recipe::WorkflowRecipe;
    use tddy_core::GoalId;
    use tddy_workflow_recipes::bugfix::BugfixRecipe;

    // Given
    let r: Arc<dyn WorkflowRecipe> = Arc::new(BugfixRecipe);
    let ctx = Context::new();
    ctx.set_sync(
        "output",
        "1. What is the expected behavior?\n2. Which version fails?",
    );

    // When
    let gate = r.host_clarification_gate_after_no_submit_turn(&GoalId::new("interview"), &ctx);

    // Then
    assert!(
        gate.is_some(),
        "BugfixRecipe must surface host recovery when prose numbered questions appear on interview goal"
    );
}

#[test]
fn bugfix_host_clarification_gate_skips_numbered_steps_without_questions() {
    use std::sync::Arc;

    use tddy_core::workflow::recipe::WorkflowRecipe;
    use tddy_core::GoalId;
    use tddy_workflow_recipes::bugfix::BugfixRecipe;

    // Given
    let r: Arc<dyn WorkflowRecipe> = Arc::new(BugfixRecipe);
    let ctx = Context::new();
    ctx.set_sync("output", "1. Clone the repository\n2. Run cargo build");

    // Then
    assert!(
        r.host_clarification_gate_after_no_submit_turn(&GoalId::new("interview"), &ctx)
            .is_none(),
        "numbered how-to steps without `?` must not trigger host recovery"
    );
}
