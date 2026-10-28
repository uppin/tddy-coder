//! `PendingAnswers`: one waiter per `request_id`, the first answer wins, and a wait ends at its
//! deadline (`#home-inbox` 6/8).

use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_discovery::subagent::{
    AnsweredBy, PermissionAnswer, PermissionAnswering, PermissionRequestView, PermissionToolCall,
};
use tddy_session_activity::pending_answers::{
    DeliveredAnswer, OpenRequest, PendingAnswers, WaitOutcome,
};

const CHILD: &str = "child-1";
const REQUEST_ID: &str = "req-1";
/// Long enough that a wait still running at the end of a test was never woken.
const A_LONG_WAIT: Duration = Duration::from_secs(30);
/// A deadline a test lets pass.
const A_SHORT_WAIT: Duration = Duration::from_millis(50);
/// How long a woken wait may take to return.
const AT_ONCE: Duration = Duration::from_secs(1);

fn a_request() -> PermissionRequestView {
    PermissionRequestView {
        request_id: REQUEST_ID.to_string(),
        tool_call: PermissionToolCall {
            title: "Bash".to_string(),
            kind: "execute".to_string(),
            raw_input: Some(serde_json::json!({ "command": "cargo publish" })),
        },
        options: Vec::new(),
    }
}

fn allow_once() -> PermissionAnswer {
    PermissionAnswer::Selected {
        request_id: REQUEST_ID.to_string(),
        option_id: "allow_once".to_string(),
    }
}

fn the_home() -> AnsweredBy {
    AnsweredBy::Orchestrator("home-1".to_string())
}

/// A registry with one waiter open for [`REQUEST_ID`].
fn with_a_waiter() -> Arc<PendingAnswers> {
    let pending = Arc::new(PendingAnswers::new());
    pending.open(CHILD, a_request());
    pending
}

async fn the_wait_ends(pending: &Arc<PendingAnswers>, deadline: Duration) -> WaitOutcome {
    tokio::time::timeout(AT_ONCE + deadline, pending.wait(REQUEST_ID, deadline))
        .await
        .expect("the wait ended")
}

#[test]
fn an_opened_request_is_open_under_its_id_with_the_session_that_asked() {
    // Given / When
    let pending = with_a_waiter();

    // Then
    assert_eq!(
        pending.open_request(REQUEST_ID),
        Some(OpenRequest {
            session_id: CHILD.to_string(),
            request: a_request(),
        })
    );
    assert_eq!(pending.pending_request(), Some(a_request()));
}

#[tokio::test]
async fn an_answer_wakes_the_wait_with_who_answered_and_the_message() {
    // Given a waiter, waiting
    let pending = with_a_waiter();
    let waiting = {
        let pending = Arc::clone(&pending);
        tokio::spawn(async move { pending.wait(REQUEST_ID, A_LONG_WAIT).await })
    };

    // When the home answers
    pending
        .answer_with_message(&allow_once(), the_home(), Some("go ahead".to_string()))
        .expect("the first answer is delivered");

    // Then the wait returns it
    let outcome = tokio::time::timeout(AT_ONCE, waiting)
        .await
        .expect("the wait was woken")
        .expect("the wait ran");
    assert_eq!(
        outcome,
        WaitOutcome::Answered(DeliveredAnswer {
            answer: allow_once(),
            by: the_home(),
            message: Some("go ahead".to_string()),
        })
    );
    // And the request is answered, and no longer open
    assert_eq!(pending.answered_by(REQUEST_ID), Some(the_home()));
    assert_eq!(pending.open_request(REQUEST_ID), None);
}

#[tokio::test]
async fn an_answer_given_before_the_wait_begins_is_the_waits_outcome() {
    // Given a waiter answered before its hook began waiting
    let pending = with_a_waiter();
    pending
        .answer_as(&allow_once(), the_home())
        .expect("the answer is delivered");

    // When the hook waits
    let outcome = the_wait_ends(&pending, A_LONG_WAIT).await;

    // Then it has the answer at once
    assert_eq!(
        outcome,
        WaitOutcome::Answered(DeliveredAnswer {
            answer: allow_once(),
            by: the_home(),
            message: None,
        })
    );
}

#[test]
fn a_second_answer_is_refused_naming_the_first_answerer() {
    // Given a request the home answered
    let pending = with_a_waiter();
    pending
        .answer_as(&allow_once(), the_home())
        .expect("the first answer is delivered");

    // When another answer arrives
    let second = pending.answer_as(
        &PermissionAnswer::Cancelled {
            request_id: REQUEST_ID.to_string(),
        },
        AnsweredBy::Orchestrator("home-2".to_string()),
    );

    // Then it is refused, and the first answerer stands
    assert_eq!(
        second,
        Err(
            "permission request 'req-1' was already answered by orchestrator session home-1"
                .to_string()
        )
    );
    assert_eq!(pending.answered_by(REQUEST_ID), Some(the_home()));
}

#[test]
fn an_answer_nobody_waits_for_is_refused_naming_the_request() {
    // Given an empty registry
    let pending = PendingAnswers::new();

    // When
    let refused = pending.answer_as(&allow_once(), the_home());

    // Then
    assert_eq!(
        refused,
        Err("no hook is waiting on permission request 'req-1'".to_string())
    );
    assert_eq!(pending.answered_by(REQUEST_ID), None);
}

#[tokio::test]
async fn a_wait_past_its_deadline_times_out_and_closes_the_request() {
    // Given a waiter nobody answers
    let pending = with_a_waiter();
    assert!(
        pending.open_request(REQUEST_ID).is_some(),
        "the request is open before its deadline"
    );

    // When its deadline passes
    let outcome = the_wait_ends(&pending, A_SHORT_WAIT).await;

    // Then it timed out, the request is closed, and a late answer is refused
    assert_eq!(outcome, WaitOutcome::TimedOut);
    assert_eq!(pending.open_request(REQUEST_ID), None);
    assert!(
        pending.answer_as(&allow_once(), the_home()).is_err(),
        "a late answer must not be delivered"
    );
}

#[test]
fn no_request_is_ever_deferred_to_the_web_user() {
    assert!(!with_a_waiter().deferred_to_user(REQUEST_ID));
}
