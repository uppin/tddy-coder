//! The notice a notification carries for a session's pending question: a field-for-field mirror of
//! the record, with the one-line summary the kernel's `QuestionNotice` names (`#home-inbox` 3/8).

use tddy_core::pending_question::{PendingQuestion, PendingQuestionItem, PendingQuestionOption};
use tddy_daemon_kernel::session_notification::{
    QuestionNotice, QuestionNoticeItem, QuestionNoticeOption,
};
use tddy_session_activity::pending_question_notice::question_notice_for;

fn a_permission_to_run(command_json: &str) -> PendingQuestion {
    PendingQuestion {
        kind: "permission".to_string(),
        question: "Run Bash: cargo publish".to_string(),
        request_id: "request-1".to_string(),
        tool_name: "Bash".to_string(),
        tool_input_json: command_json.to_string(),
        at_unix_ms: 1_761_000_000_000,
        ..Default::default()
    }
}

fn a_two_question_record() -> PendingQuestion {
    PendingQuestion {
        kind: "select".to_string(),
        header: "Language".to_string(),
        question: "Which language?".to_string(),
        options: vec!["Rust".to_string()],
        allow_other: true,
        request_id: "request-2".to_string(),
        tool_name: "AskUserQuestion".to_string(),
        tool_input_json: "{}".to_string(),
        questions: vec![
            PendingQuestionItem {
                header: "Language".to_string(),
                question: "Which language?".to_string(),
                options: vec![PendingQuestionOption {
                    label: "Rust".to_string(),
                    description: "fast".to_string(),
                }],
                multi_select: false,
            },
            PendingQuestionItem {
                header: "Targets".to_string(),
                question: "Which targets?".to_string(),
                options: vec![
                    PendingQuestionOption {
                        label: "linux".to_string(),
                        description: String::new(),
                    },
                    PendingQuestionOption {
                        label: "macos".to_string(),
                        description: "aarch64".to_string(),
                    },
                ],
                multi_select: true,
            },
        ],
        at_unix_ms: 1_761_000_000_000,
    }
}

#[test]
fn a_permission_becomes_a_notice_naming_its_tool_and_input_summary() {
    // Given a permission to run `cargo publish`
    let pending = a_permission_to_run(r#"{"command":"cargo publish"}"#);

    // When
    let notice = question_notice_for(&pending);

    // Then
    assert_eq!(
        notice,
        QuestionNotice {
            request_id: "request-1".to_string(),
            kind: "permission".to_string(),
            tool_name: "Bash".to_string(),
            summary: "cargo publish".to_string(),
            questions: Vec::new(),
        }
    );
}

#[test]
fn a_question_becomes_a_notice_carrying_every_question_with_its_options() {
    // Given an AskUserQuestion call with two questions
    let pending = a_two_question_record();

    // When
    let notice = question_notice_for(&pending);

    // Then the summary is the first question, and every question and option is carried
    assert_eq!(
        notice,
        QuestionNotice {
            request_id: "request-2".to_string(),
            kind: "select".to_string(),
            tool_name: "AskUserQuestion".to_string(),
            summary: "Which language?".to_string(),
            questions: vec![
                QuestionNoticeItem {
                    header: "Language".to_string(),
                    question: "Which language?".to_string(),
                    options: vec![QuestionNoticeOption {
                        label: "Rust".to_string(),
                        description: "fast".to_string(),
                    }],
                    multi_select: false,
                },
                QuestionNoticeItem {
                    header: "Targets".to_string(),
                    question: "Which targets?".to_string(),
                    options: vec![
                        QuestionNoticeOption {
                            label: "linux".to_string(),
                            description: String::new(),
                        },
                        QuestionNoticeOption {
                            label: "macos".to_string(),
                            description: "aarch64".to_string(),
                        },
                    ],
                    multi_select: true,
                },
            ],
        }
    );
}
