//! The one line a [`PendingQuestion`] is summarised by: what a permission asks to run, or what a
//! question asks (`#home-inbox` 3/8).

use rstest::rstest;
use tddy_changeset::pending_question::{PendingQuestion, PendingQuestionItem};

/// A claude-cli permission prompt for `tool_name`, with `tool_input_json` as the hook sent it.
fn a_permission_for(tool_name: &str, tool_input_json: &str) -> PendingQuestion {
    PendingQuestion {
        kind: "permission".to_string(),
        tool_name: tool_name.to_string(),
        tool_input_json: tool_input_json.to_string(),
        ..Default::default()
    }
}

/// An `AskUserQuestion` call whose questions are `texts`, the flat fields mirroring the first.
fn a_question_asking(texts: &[&str]) -> PendingQuestion {
    PendingQuestion {
        kind: "select".to_string(),
        question: texts[0].to_string(),
        tool_name: "AskUserQuestion".to_string(),
        questions: texts
            .iter()
            .map(|text| PendingQuestionItem {
                question: (*text).to_string(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[rstest]
#[case::a_bash_command_is_what_it_runs(
    "Bash",
    r#"{"command":"cargo publish","description":"Publish"}"#,
    "cargo publish"
)]
#[case::an_edit_is_the_file_it_changes(
    "Edit",
    r#"{"file_path":"src/lib.rs","old_string":"a","new_string":"b"}"#,
    "src/lib.rs"
)]
#[case::a_write_is_the_file_it_creates(
    "Write",
    r#"{"file_path":"notes.md","content":"hello"}"#,
    "notes.md"
)]
#[case::a_read_is_the_file_it_opens("Read", r#"{"file_path":"/etc/hosts"}"#, "/etc/hosts")]
#[case::any_other_tool_is_its_input_as_compact_json(
    "WebFetch",
    r#"{ "url": "https://example.com" }"#,
    r#"{"url":"https://example.com"}"#
)]
fn a_permission_is_summarised_by_what_it_asks_to_do(
    #[case] tool_name: &str,
    #[case] tool_input_json: &str,
    #[case] expected: &str,
) {
    // Given a permission prompt for the tool
    let permission = a_permission_for(tool_name, tool_input_json);

    // When
    let summary = permission.summary();

    // Then
    assert_eq!(summary, expected);
}

#[test]
fn a_question_is_summarised_by_its_first_question() {
    // Given an AskUserQuestion call with two questions
    let question = a_question_asking(&["Which language?", "Which targets?"]);

    // When
    let summary = question.summary();

    // Then
    assert_eq!(summary, "Which language?");
}

#[test]
fn a_multi_select_question_is_summarised_the_same_way() {
    // Given
    let mut question = a_question_asking(&["Which targets?"]);
    question.kind = "multi_select".to_string();

    // When
    let summary = question.summary();

    // Then
    assert_eq!(summary, "Which targets?");
}

#[test]
fn a_record_that_names_no_tool_input_and_no_question_has_no_summary() {
    // Given the message-only record a bare Notification hook leaves
    let record = PendingQuestion {
        kind: "permission".to_string(),
        question: "Claude needs your permission to use Bash".to_string(),
        ..Default::default()
    };

    // When
    let summary = record.summary();

    // Then
    assert_eq!(summary, "");
}
