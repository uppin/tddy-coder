//! How a pending question is laid out as one Telegram HTML message: the permission and question
//! layouts, and the escaping of the session's own words (`#home-inbox` 3/8).
//!
//! The alert is a pure function of the notification, so these call the renderer directly; the path
//! from the bus to the chat is `question_alert_acceptance.rs`.

use tddy_daemon_kernel::session_notification::{
    QuestionNotice, QuestionNoticeItem, QuestionNoticeOption, SessionNotification,
    SessionNotificationKind, SessionNotificationSource,
};
use tddy_telegram_control::question_alert::QuestionAlert;

/// A `PendingQuestion` notification for the session labelled `label`, carrying `question`.
fn a_notification_for(label: &str, question: QuestionNotice) -> SessionNotification {
    SessionNotification {
        session_id: "01900000-0000-7000-8000-AABB00000701".to_string(),
        os_user: "ada".to_string(),
        label: label.to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::PendingQuestion,
        text: "asks".to_string(),
        agent: None,
        question: Some(question),
        at_unix_ms: 1_761_000_000_000,
    }
}

fn a_permission(tool_name: &str, summary: &str) -> QuestionNotice {
    QuestionNotice {
        request_id: "request-1".to_string(),
        kind: "permission".to_string(),
        tool_name: tool_name.to_string(),
        summary: summary.to_string(),
        questions: Vec::new(),
    }
}

fn a_question_of(items: Vec<QuestionNoticeItem>) -> QuestionNotice {
    QuestionNotice {
        request_id: "request-2".to_string(),
        kind: "select".to_string(),
        tool_name: "AskUserQuestion".to_string(),
        summary: items[0].question.clone(),
        questions: items,
    }
}

fn an_item(header: &str, question: &str, options: &[(&str, &str)]) -> QuestionNoticeItem {
    QuestionNoticeItem {
        header: header.to_string(),
        question: question.to_string(),
        options: options
            .iter()
            .map(|(label, description)| QuestionNoticeOption {
                label: (*label).to_string(),
                description: (*description).to_string(),
            })
            .collect(),
        multi_select: false,
    }
}

fn html_of(notification: &SessionNotification) -> String {
    QuestionAlert::render(notification)
        .expect("a notification carrying a question renders an alert")
        .html()
        .to_string()
}

#[test]
fn a_permission_reads_session_wants_to_run_tool_and_input_summary() {
    // Given
    let notification = a_notification_for("my-branch", a_permission("Bash", "cargo publish"));

    // When
    let html = html_of(&notification);

    // Then
    assert_eq!(
        html,
        "<i>Session my-branch wants to run</i> <code>Bash</code>: <code>cargo publish</code>"
    );
}

#[test]
fn what_the_session_wrote_is_escaped_but_quotes_are_left_alone() {
    // Given a label, a tool and a command carrying every character Telegram's HTML mode reads
    let notification =
        a_notification_for("a<b>&c", a_permission("Mcp&Tool", r#"echo "<tag>" && ls"#));

    // When
    let html = html_of(&notification);

    // Then
    assert_eq!(
        html,
        "<i>Session a&lt;b&gt;&amp;c wants to run</i> <code>Mcp&amp;Tool</code>: <code>echo \"&lt;tag&gt;\" &amp;&amp; ls</code>"
    );
}

#[test]
fn a_question_lists_its_header_text_and_options_with_descriptions() {
    // Given one question with two options, one without a description
    let notification = a_notification_for(
        "my-branch",
        a_question_of(vec![an_item(
            "Language",
            "Which language?",
            &[("Rust", "Memory safe"), ("Go", "")],
        )]),
    );

    // When
    let html = html_of(&notification);

    // Then
    assert_eq!(
        html,
        "<i>Session my-branch asks</i>\n\n<b>Language</b>\nWhich language?\n\n• Rust: Memory safe\n• Go"
    );
}

#[test]
fn a_question_without_a_header_has_no_header_line() {
    // Given
    let notification = a_notification_for(
        "my-branch",
        a_question_of(vec![an_item("", "Which language?", &[("Rust", "")])]),
    );

    // When
    let html = html_of(&notification);

    // Then
    assert_eq!(
        html,
        "<i>Session my-branch asks</i>\n\nWhich language?\n\n• Rust"
    );
}

#[test]
fn a_multi_select_question_says_several_can_be_chosen() {
    // Given
    let mut item = an_item("Targets", "Which targets?", &[("linux", ""), ("macos", "")]);
    item.multi_select = true;
    let notification = a_notification_for("my-branch", a_question_of(vec![item]));

    // When
    let html = html_of(&notification);

    // Then
    assert_eq!(
        html,
        "<i>Session my-branch asks</i>\n\n<b>Targets</b>\nWhich targets? (choose several)\n\n• linux\n• macos"
    );
}

#[test]
fn several_questions_follow_one_another_separated_by_a_blank_line() {
    // Given
    let notification = a_notification_for(
        "my-branch",
        a_question_of(vec![
            an_item("Language", "Which language?", &[("Rust", "")]),
            an_item("Cloud", "Which cloud?", &[("aws", ""), ("gcp", "")]),
        ]),
    );

    // When
    let html = html_of(&notification);

    // Then
    assert_eq!(
        html,
        "<i>Session my-branch asks</i>\n\n<b>Language</b>\nWhich language?\n\n• Rust\n\n<b>Cloud</b>\nWhich cloud?\n\n• aws\n• gcp"
    );
}

#[test]
fn the_questions_own_words_are_escaped() {
    // Given a question, an option and a description with markup in them
    let notification = a_notification_for(
        "my-branch",
        a_question_of(vec![an_item(
            "A&B",
            "Use <T> or &U?",
            &[("Vec<T>", "a & b")],
        )]),
    );

    // When
    let html = html_of(&notification);

    // Then
    assert_eq!(
        html,
        "<i>Session my-branch asks</i>\n\n<b>A&amp;B</b>\nUse &lt;T&gt; or &amp;U?\n\n• Vec&lt;T&gt;: a &amp; b"
    );
}
