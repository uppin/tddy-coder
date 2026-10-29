//! The pieces of answering an `AskUserQuestion` (`#home-inbox` 8/9), one at a time: validating
//! answers against the recorded questions, the refusals' wording, the question waiters of
//! `PendingAnswers`, and the record the child's activity keeps of an answer.
//!
//! The end-to-end behaviour is `question_answer_acceptance.rs`.

use std::time::Duration;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_core::pending_question::{PendingQuestion, PendingQuestionItem, PendingQuestionOption};
use tddy_discovery::subagent::AnsweredBy;
use tddy_session_activity::answers::{
    answered_question_output, question_tool_call_id, validate_question_answers, AnswerRefusal,
    QuestionAnswer,
};
use tddy_session_activity::pending_answers::{
    DeliveredQuestionAnswers, PendingAnswers, QuestionWaitOutcome,
};

const REQUEST_ID: &str = "req-which-language";
const CHILD: &str = "child-session-1";
const HOME: &str = "home-session-1";
/// A deadline no waiter is expected to reach.
const A_LONG_WAIT: Duration = Duration::from_secs(30);
const A_SHORT_WAIT: Duration = Duration::from_millis(50);
/// What a woken or expired wait may take: bounds every test.
const AT_ONCE: Duration = Duration::from_secs(1);

fn item(question: &str, labels: &[&str], multi_select: bool) -> PendingQuestionItem {
    PendingQuestionItem {
        header: String::new(),
        question: question.to_string(),
        options: labels
            .iter()
            .map(|label| PendingQuestionOption {
                label: label.to_string(),
                description: String::new(),
            })
            .collect(),
        multi_select,
    }
}

/// An `AskUserQuestion` of a single-choice question (Rust or Go) and a multi-select one.
fn the_two_questions() -> PendingQuestion {
    PendingQuestion {
        kind: "select".to_string(),
        question: "Which language?".to_string(),
        request_id: REQUEST_ID.to_string(),
        tool_name: "AskUserQuestion".to_string(),
        questions: vec![
            item("Which language?", &["Rust", "Go"], false),
            item(
                "Which checks should run?",
                &["fmt", "clippy", "tests"],
                true,
            ),
        ],
        ..Default::default()
    }
}

fn answer(question_index: u32, selected: &[&str], other: Option<&str>) -> QuestionAnswer {
    QuestionAnswer {
        question_index,
        selected: selected.iter().map(|label| label.to_string()).collect(),
        other: other.map(str::to_string),
    }
}

// ---------------------------------------------------------------------------
// validate_question_answers
// ---------------------------------------------------------------------------

#[rstest]
#[case::one_label_for_the_single_choice(vec![answer(0, &["Rust"], None)])]
#[case::a_multi_select_with_several_labels(vec![answer(1, &["fmt", "clippy", "tests"], None)])]
#[case::both_questions(vec![answer(0, &["Go"], None), answer(1, &["fmt"], None)])]
#[case::free_text_instead_of_a_label(vec![answer(0, &[], Some("Zig"))])]
#[case::a_multi_select_with_labels_and_free_text(vec![answer(1, &["fmt"], Some("bench"))])]
#[case::only_the_second_question(vec![answer(1, &["tests"], None)])]
fn answers_that_fit_the_questions_are_accepted(#[case] answers: Vec<QuestionAnswer>) {
    // When
    let verdict = validate_question_answers(&the_two_questions(), &answers);

    // Then
    assert_eq!(verdict, Ok(()));
}

#[rstest]
#[case::an_index_the_call_does_not_have(vec![answer(2, &["Rust"], None)], "2")]
#[case::an_unoffered_label(vec![answer(0, &["Python"], None)], "Python")]
#[case::a_label_spelled_differently(vec![answer(0, &["rust"], None)], "rust")]
#[case::two_labels_on_a_single_choice(vec![answer(0, &["Rust", "Go"], None)], "one")]
#[case::a_label_and_free_text_on_a_single_choice(vec![answer(0, &["Rust"], Some("Zig"))], "one")]
#[case::the_same_question_twice(
    vec![answer(0, &["Rust"], None), answer(0, &["Go"], None)],
    "twice"
)]
#[case::an_answer_that_selects_nothing(vec![answer(1, &[], None)], "nothing")]
#[case::an_unoffered_label_among_valid_ones(vec![answer(1, &["fmt", "bench"], None)], "bench")]
fn answers_that_do_not_fit_are_refused_saying_why(
    #[case] answers: Vec<QuestionAnswer>,
    #[case] mentions: &str,
) {
    // When
    let verdict = validate_question_answers(&the_two_questions(), &answers);

    // Then
    let reason = verdict.expect_err("the answers do not fit");
    assert!(
        reason.contains(mentions),
        "refused, but the reason does not mention {mentions:?}: {reason}"
    );
}

#[test]
fn a_record_with_no_structured_questions_refuses_every_index() {
    // Given a pending question that carries only its flat fields
    let flat = PendingQuestion {
        kind: "select".to_string(),
        question: "Which language?".to_string(),
        options: vec!["Rust".to_string(), "Go".to_string()],
        request_id: REQUEST_ID.to_string(),
        ..Default::default()
    };

    // When
    let verdict = validate_question_answers(&flat, &[answer(0, &["Rust"], None)]);

    // Then
    let reason = verdict.expect_err("there is no question 0 to answer");
    assert!(reason.contains('0'), "the reason names the index: {reason}");
}

// ---------------------------------------------------------------------------
// The refusals' wording
// ---------------------------------------------------------------------------

#[test]
fn not_a_question_names_the_tool_that_answers_a_permission() {
    // When
    let text = AnswerRefusal::NotAQuestion {
        request_id: REQUEST_ID.to_string(),
        kind: "permission".to_string(),
    }
    .to_string();

    // Then
    for needle in [REQUEST_ID, "permission", "AnswerSessionPermission"] {
        assert!(text.contains(needle), "{needle:?} missing from: {text}");
    }
}

#[test]
fn invalid_answers_names_the_request_and_the_reason() {
    // When
    let text = AnswerRefusal::InvalidAnswers {
        request_id: REQUEST_ID.to_string(),
        reason: "question 0 does not offer \"Python\"".to_string(),
    }
    .to_string();

    // Then
    for needle in [REQUEST_ID, "question 0 does not offer \"Python\""] {
        assert!(text.contains(needle), "{needle:?} missing from: {text}");
    }
}

#[test]
fn no_request_id_names_the_session_and_why_it_cannot_be_answered_here() {
    // When
    let text = AnswerRefusal::NoRequestId {
        session_id: CHILD.to_string(),
    }
    .to_string();

    // Then
    for needle in [CHILD, "request_id", "presenter", "AnswerSessionQuestion"] {
        assert!(text.contains(needle), "{needle:?} missing from: {text}");
    }
}

// ---------------------------------------------------------------------------
// The question waiters
// ---------------------------------------------------------------------------

fn rust_with_fmt() -> Vec<QuestionAnswer> {
    vec![answer(0, &["Rust"], None), answer(1, &["fmt"], None)]
}

fn the_home() -> AnsweredBy {
    AnsweredBy::Orchestrator(HOME.to_string())
}

#[tokio::test]
async fn an_open_question_is_visible_and_an_answer_wakes_its_waiter() {
    // Given a hook waiting on the question
    let pending = std::sync::Arc::new(PendingAnswers::new());
    pending.open_question(CHILD, the_two_questions());
    let open = pending
        .open_question_request(REQUEST_ID)
        .expect("the waiter is open");
    assert_eq!(
        (open.session_id.as_str(), open.question),
        (CHILD, the_two_questions())
    );
    let waiter = tokio::spawn({
        let pending = std::sync::Arc::clone(&pending);
        async move { pending.wait_for_question(REQUEST_ID, A_LONG_WAIT).await }
    });

    // When the home answers
    pending
        .answer_question_with(REQUEST_ID, rust_with_fmt(), the_home())
        .expect("the waiter was open");

    // Then the wait ends with those answers and who gave them
    let outcome = tokio::time::timeout(AT_ONCE, waiter)
        .await
        .expect("the waiter was woken")
        .expect("the wait ran to completion");
    assert_eq!(
        outcome,
        QuestionWaitOutcome::Answered(DeliveredQuestionAnswers {
            answers: rust_with_fmt(),
            by: the_home(),
        })
    );
    assert_eq!(pending.question_answered_by(REQUEST_ID), Some(the_home()));
}

#[tokio::test]
async fn an_answer_that_arrives_before_the_wait_is_the_waits_outcome() {
    // Given an open question answered before the hook starts waiting
    let pending = PendingAnswers::new();
    pending.open_question(CHILD, the_two_questions());
    pending
        .answer_question_with(REQUEST_ID, rust_with_fmt(), the_home())
        .expect("the waiter was open");

    // When the hook waits
    let outcome = tokio::time::timeout(AT_ONCE, pending.wait_for_question(REQUEST_ID, A_LONG_WAIT))
        .await
        .expect("an answered question does not wait");

    // Then
    assert_eq!(
        outcome,
        QuestionWaitOutcome::Answered(DeliveredQuestionAnswers {
            answers: rust_with_fmt(),
            by: the_home(),
        })
    );
}

#[tokio::test]
async fn the_first_answer_wins_and_a_second_is_refused_naming_the_first() {
    // Given a question the home has answered
    let pending = PendingAnswers::new();
    pending.open_question(CHILD, the_two_questions());
    pending
        .answer_question_with(REQUEST_ID, rust_with_fmt(), the_home())
        .expect("the waiter was open");

    // When another session answers it
    let refused = pending.answer_question_with(
        REQUEST_ID,
        vec![answer(0, &["Go"], None)],
        AnsweredBy::Orchestrator("other-session".to_string()),
    );

    // Then it is refused, naming the first answerer
    let reason = refused.expect_err("the question was already answered");
    for needle in ["already answered", HOME] {
        assert!(reason.contains(needle), "{needle:?} missing from: {reason}");
    }
}

#[tokio::test]
async fn a_wait_that_times_out_removes_the_waiter_and_a_late_answer_is_refused() {
    // Given a hook that waits a moment
    let pending = PendingAnswers::new();
    pending.open_question(CHILD, the_two_questions());

    // When nobody answers
    let outcome =
        tokio::time::timeout(AT_ONCE, pending.wait_for_question(REQUEST_ID, A_SHORT_WAIT))
            .await
            .expect("the wait ends at its deadline");

    // Then it timed out, the waiter is gone, and an answer arriving after it is refused
    assert_eq!(outcome, QuestionWaitOutcome::TimedOut);
    assert_eq!(pending.open_question_request(REQUEST_ID), None);
    let reason = pending
        .answer_question_with(REQUEST_ID, rust_with_fmt(), the_home())
        .expect_err("nobody is waiting");
    assert!(
        reason.contains("no hook is waiting"),
        "the refusal says nobody waits: {reason}"
    );
}

#[tokio::test]
async fn waiting_on_a_question_nobody_opened_times_out_at_once() {
    // Given no waiter
    let pending = PendingAnswers::new();

    // When a hook waits on an unknown request
    let outcome =
        tokio::time::timeout(AT_ONCE, pending.wait_for_question("req-never", A_LONG_WAIT))
            .await
            .expect("an unopened question does not wait");

    // Then
    assert_eq!(outcome, QuestionWaitOutcome::TimedOut);
}

#[test]
fn a_question_waiter_is_not_a_permission_waiter() {
    // Given a question open under REQUEST_ID
    let pending = PendingAnswers::new();
    pending.open_question(CHILD, the_two_questions());

    // Then the permission side does not see it
    assert_eq!(pending.open_request(REQUEST_ID), None);
    // And the question side does
    assert!(pending.open_question_request(REQUEST_ID).is_some());
}

// ---------------------------------------------------------------------------
// What the child's activity records
// ---------------------------------------------------------------------------

#[test]
fn the_child_records_the_answer_under_question_colon_the_request_id() {
    assert_eq!(
        question_tool_call_id(REQUEST_ID),
        "question:req-which-language"
    );
}

#[test]
fn the_recorded_output_names_who_answered_and_each_answer_leaving_out_absent_free_text() {
    // When
    let output = answered_question_output(
        HOME,
        &[
            answer(0, &[], Some("Zig")),
            answer(1, &["fmt", "clippy"], None),
        ],
    );

    // Then
    assert_eq!(
        output,
        serde_json::json!({
            "answeredBy": HOME,
            "answers": [
                { "questionIndex": 0, "selected": [], "other": "Zig" },
                { "questionIndex": 1, "selected": ["fmt", "clippy"] }
            ]
        })
    );
}
