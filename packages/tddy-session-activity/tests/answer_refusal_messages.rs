//! What an orchestrator is told when its answer is refused (`#home-inbox` 6/8). The text is what the
//! agent reads in the tool's error, so each names the remedy.

use pretty_assertions::assert_eq;
use tddy_session_activity::answers::AnswerRefusal;

#[test]
fn a_caller_that_is_not_the_orchestrator_is_told_permission_denied_and_who_is() {
    let refusal = AnswerRefusal::NotTheOrchestrator {
        caller: "other-home".to_string(),
        target: "child-1".to_string(),
        orchestrator: Some("home-1".to_string()),
    };

    assert_eq!(
        refusal.to_string(),
        "permission denied: session other-home is not the orchestrator of session child-1 \
         (its orchestrator is home-1)"
    );
}

#[test]
fn a_child_with_no_orchestrator_says_none() {
    let refusal = AnswerRefusal::NotTheOrchestrator {
        caller: "home-1".to_string(),
        target: "child-1".to_string(),
        orchestrator: None,
    };

    assert_eq!(
        refusal.to_string(),
        "permission denied: session home-1 is not the orchestrator of session child-1 \
         (its orchestrator is none)"
    );
}

#[test]
fn a_second_answer_names_who_answered_first() {
    let refusal = AnswerRefusal::AlreadyAnswered {
        request_id: "req-1".to_string(),
        by: "home-1".to_string(),
    };

    assert_eq!(
        refusal.to_string(),
        "permission request 'req-1' was already answered by home-1"
    );
}

#[test]
fn an_unknown_request_names_it() {
    let refusal = AnswerRefusal::UnknownRequest {
        request_id: "req-nope".to_string(),
    };

    assert_eq!(
        refusal.to_string(),
        "no permission request 'req-nope' is pending on that session"
    );
}

#[test]
fn a_question_points_at_answer_session_question() {
    let refusal = AnswerRefusal::NotAPermission {
        request_id: "req-1".to_string(),
        kind: "select".to_string(),
    };

    assert_eq!(
        refusal.to_string(),
        "pending question 'req-1' is a select question, not a permission: answer it with \
         AnswerSessionQuestion"
    );
}

#[test]
fn an_option_the_request_does_not_offer_names_it() {
    let refusal = AnswerRefusal::UnknownOption {
        request_id: "req-1".to_string(),
        option_id: "allow_always".to_string(),
    };

    assert_eq!(
        refusal.to_string(),
        "permission request 'req-1' offers no option 'allow_always'"
    );
}
