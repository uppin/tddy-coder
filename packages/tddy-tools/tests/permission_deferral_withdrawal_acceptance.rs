//! What `subagent_resume` advertises about a deferred permission request: the main agent may still
//! answer it itself, the first answer wins, and an answer sent while the user is answering is refused
//! as delivered rather than queued. The advertised text has to say what the call does.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

use tddy_tools::permission_answer_arg::permission_property;
use tddy_tools::permission_deferral_arg::defer_property;

fn description_of(property: &serde_json::Value) -> String {
    property["description"]
        .as_str()
        .expect("the property is described")
        .to_string()
}

#[test]
fn the_deferral_says_the_main_agent_may_still_answer_and_the_first_answer_wins() {
    // Given the advertised deferral
    let description = description_of(&defer_property());

    // Then it says the caller may still answer the request itself, with `permission`
    assert!(
        description.contains("`permission`"),
        "the deferral names the answer that still reaches the request: {description}"
    );

    // And that the first answer wins
    assert!(
        description.contains("first answer wins"),
        "the deferral says which answer counts: {description}"
    );

    // And it does not claim the request cannot be answered while the user answers it
    assert!(
        !description.contains("cannot answer"),
        "the deferral must not say the caller is shut out: {description}"
    );
}

#[test]
fn the_permission_answer_says_an_answer_sent_while_the_user_answers_is_refused_as_delivered() {
    // Given the advertised answer
    let description = description_of(&permission_property());

    // Then it says what a call sent while the request is with the user does
    assert!(
        description.contains("delivered at once"),
        "the answer says it is delivered at once while the user answers: {description}"
    );
    assert!(
        description.contains("refused"),
        "the answer says the call is refused, not continued: {description}"
    );
}
