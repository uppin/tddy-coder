//! The orchestrator's role prompt (`#orchestra` 9/9): who it is, that coding goes through tddy
//! sessions it follows, that its children report back, and every tool it is offered.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Role prompt.

use pretty_assertions::assert_eq;

use super::project_home_prompt;

const MAIN: &str = "project-home";
const ORCHESTRATOR: &str = "orchestrator";

fn first_line(prompt: &str) -> &str {
    prompt
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
}

#[test]
fn an_orchestrators_prompt_opens_as_an_orchestrator_and_mains_as_main() {
    // When
    let (orchestrator, main) = (project_home_prompt(ORCHESTRATOR), project_home_prompt(MAIN));

    // Then
    assert!(
        first_line(&orchestrator).contains("orchestrator"),
        "{orchestrator}"
    );
    assert!(
        !first_line(&orchestrator).contains("main session"),
        "{orchestrator}"
    );
    assert!(first_line(&main).contains("main session"), "{main}");
}

#[test]
fn the_prompt_sends_coding_work_through_sessions_it_starts_and_follows() {
    // When
    let prompt = project_home_prompt(ORCHESTRATOR);

    // Then it says to start a session for coding and to follow it with watches until it ends
    for needed in [
        "StartSession",
        "CreateSessionWatch",
        "WaitSessionWatchEvents",
        "ReadSessionTranscript",
    ] {
        assert!(prompt.contains(needed), "the prompt does not name {needed}");
    }
}

#[test]
fn the_prompt_has_children_report_progress_and_status_over_their_back_channel() {
    // When
    let prompt = project_home_prompt(ORCHESTRATOR);

    // Then
    assert!(
        prompt.contains("progress") && prompt.contains("status") && prompt.contains("back channel"),
        "{prompt}"
    );
}

#[test]
fn the_prompt_checks_the_base_before_work_and_reads_trunk_activities() {
    // When
    let prompt = project_home_prompt(ORCHESTRATOR);

    // Then
    for needed in ["CheckBaseFreshness", "RefreshHome", "trunk_commit"] {
        assert!(prompt.contains(needed), "the prompt does not name {needed}");
    }
}

#[test]
fn every_tool_the_orchestrator_is_offered_is_named_in_its_prompt() {
    // Given every tool an orchestrator-role session is offered
    let prompt = project_home_prompt(ORCHESTRATOR);

    // When the prompt is searched for each
    let unnamed: Vec<String> = tddy_tool_engine::project_home_tools::project_home_offered_catalog()
        .into_iter()
        .map(|tool| tool.name)
        .filter(|name| !prompt.contains(name.as_str()))
        .collect();

    // Then none is missing
    assert_eq!(unnamed, Vec::<String>::new());
}
