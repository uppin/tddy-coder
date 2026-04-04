//! Acceptance (PRD Testing Plan): interview must elicit demo participation/options via `tddy-tools ask`.

use tddy_workflow_recipes::tdd::interview;

/// PRD: system and user prompts must mandate demo yes/no and options elicitation (no silent skip).
#[test]
fn interview_prompt_requires_demo_elicitation() {
    let system = interview::system_prompt();
    let user = interview::build_interview_user_prompt("Example feature for demo routing");

    let combined = format!("{system}\n{user}");
    for needle in [
        "demo",
        "tddy-tools ask",
        "run_optional_step_x",
        "demo_options",
    ] {
        assert!(
            combined.contains(needle),
            "interview prompts must require demo elicitation and persistence contract; missing {needle:?}.\n---\n{combined}\n---"
        );
    }
}
