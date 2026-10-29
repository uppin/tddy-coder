//! What `session-hook --await-answer` prints for an orchestrator's answers to an `AskUserQuestion`:
//! Claude Code's `PreToolUse` decision, `permissionDecision: allow` with the call's own input
//! plus an `answers` map (`#home-inbox` 8/9). The mechanism was verified against Claude Code
//! 2.1.312; see `question_decision.rs`.

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_tools::session_hook::question_decision::encode_question_decision;
use tddy_tools::session_hook::QuestionAnswer;

/// A single-choice and a multi-select question, as Claude sends them.
fn the_input() -> Value {
    json!({
        "questions": [
            {"question": "Which language?", "header": "Language", "multiSelect": false,
             "options": [{"label": "Rust", "description": ""}, {"label": "Go", "description": ""}]},
            {"question": "Which checks should run?", "header": "Checks", "multiSelect": true,
             "options": [{"label": "fmt", "description": ""}, {"label": "clippy", "description": ""}]}
        ]
    })
}

fn answer(question_index: u32, selected: &[&str], other: Option<&str>) -> QuestionAnswer {
    QuestionAnswer {
        question_index,
        selected: selected.iter().map(|label| label.to_string()).collect(),
        other: other.map(str::to_string),
    }
}

/// The decision Claude reads, when `answers` is what the call is answered with.
fn decision(answers: Value) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": {
                "questions": the_input()["questions"],
                "answers": answers
            }
        }
    })
}

fn printed(input: &Value, answers: &[QuestionAnswer]) -> Option<Value> {
    encode_question_decision(input, answers)
        .map(|stdout| serde_json::from_str(&stdout).expect("the decision is JSON"))
}

#[test]
fn a_single_choice_answer_is_the_label_keyed_by_the_question_text() {
    assert_eq!(
        printed(&the_input(), &[answer(0, &["Rust"], None)]),
        Some(decision(json!({ "Which language?": "Rust" })))
    );
}

#[test]
fn a_multi_select_answer_joins_the_labels_with_comma_space() {
    assert_eq!(
        printed(&the_input(), &[answer(1, &["fmt", "clippy"], None)]),
        Some(decision(
            json!({ "Which checks should run?": "fmt, clippy" })
        ))
    );
}

#[test]
fn free_text_alone_is_the_answer() {
    assert_eq!(
        printed(&the_input(), &[answer(0, &[], Some("Zig"))]),
        Some(decision(json!({ "Which language?": "Zig" })))
    );
}

#[test]
fn labels_and_free_text_are_joined_with_the_free_text_last() {
    assert_eq!(
        printed(
            &the_input(),
            &[answer(1, &["fmt", "clippy"], Some("bench"))]
        ),
        Some(decision(
            json!({ "Which checks should run?": "fmt, clippy, bench" })
        ))
    );
}

#[test]
fn a_question_the_answers_leave_out_is_absent_from_the_map() {
    // When only the second question is answered
    let printed = printed(&the_input(), &[answer(1, &["fmt"], None)]).expect("a decision");

    // Then the first is not in `answers`, and Claude asks it
    assert_eq!(
        printed["hookSpecificOutput"]["updatedInput"]["answers"],
        json!({ "Which checks should run?": "fmt" })
    );
}

#[test]
fn the_call_s_own_input_is_kept_beside_the_answers() {
    // Given an input with a field besides `questions`
    let mut input = the_input();
    input["metadata"] = json!({ "source": "plan" });

    // When
    let printed = printed(&input, &[answer(0, &["Go"], None)]).expect("a decision");

    // Then `updatedInput`, which replaces the input whole, still carries it
    assert_eq!(
        printed["hookSpecificOutput"]["updatedInput"]["metadata"],
        json!({ "source": "plan" })
    );
}

#[test]
fn no_answers_print_nothing() {
    assert_eq!(printed(&the_input(), &[]), None);
}

#[test]
fn an_answer_for_a_question_the_call_does_not_have_prints_nothing() {
    assert_eq!(printed(&the_input(), &[answer(5, &["Rust"], None)]), None);
}

#[test]
fn an_input_without_questions_prints_nothing() {
    assert_eq!(
        printed(&json!({ "prompt": "hello" }), &[answer(0, &["Rust"], None)]),
        None
    );
}
