//! `NotifyOperator`'s definition and arguments (`#home-inbox` 11/11). Its dispatch runs through
//! `ProjectHomeNotifyPort::notify_operator`, which `tddy-daemon-rpc`'s
//! `notify_operator_tool_acceptance.rs` exercises end to end.
//!
//! The limits are `tddy-tools notify`'s (`tddy_daemon_kernel::agent_notification_limits`): a
//! standing-role session is refused exactly what the CLI is.

use pretty_assertions::assert_eq;
use tddy_daemon_kernel::agent_notification_limits::{MAX_BODY_CHARS, MAX_TITLE_CHARS};
use tddy_service::proto::activity::NotificationLevel;
use tddy_tool_engine::project_home_notify_tools::{
    is_project_home_notify_tool, notify_operator_tool_def, NotifyOperatorArgs, OperatorNotice,
    NOTIFY_OPERATOR,
};

fn args(title: &str, body_markdown: &str) -> NotifyOperatorArgs {
    NotifyOperatorArgs {
        title: title.to_string(),
        body_markdown: body_markdown.to_string(),
        level: None,
        sound: None,
    }
}

fn refusal_of(args: NotifyOperatorArgs) -> String {
    args.into_notice().expect_err("the arguments are refused")
}

#[test]
fn notify_operator_is_the_notify_tool_and_requires_a_title_and_a_body() {
    // Given the tool's definition
    let def = notify_operator_tool_def();
    let schema: serde_json::Value =
        serde_json::from_str(&def.input_schema_json).expect("the schema is JSON");

    // Then it is the notify tool, requiring exactly the title and the body
    assert!(is_project_home_notify_tool(NOTIFY_OPERATOR));
    assert!(!is_project_home_notify_tool("ReadSessionActivity"));
    assert_eq!(def.name, NOTIFY_OPERATOR);
    assert_eq!(
        schema["required"],
        serde_json::json!(["title", "body_markdown"])
    );
}

#[test]
fn a_title_and_body_with_no_level_or_sound_are_a_silent_info_notice() {
    // Given only a title and a body
    // When they are read
    let notice = args("Stack green", "**3 of 4** merged").into_notice();

    // Then it is an info notice, silent
    assert_eq!(
        notice,
        Ok(OperatorNotice {
            title: "Stack green".to_string(),
            body_markdown: "**3 of 4** merged".to_string(),
            level: NotificationLevel::Info,
            sound: false,
        })
    );
}

#[test]
fn each_named_level_and_the_sound_are_carried_through() {
    for (name, level) in [
        ("info", NotificationLevel::Info),
        ("success", NotificationLevel::Success),
        ("warning", NotificationLevel::Warning),
        ("error", NotificationLevel::Error),
    ] {
        // Given a level by name, with a sound
        let asked = NotifyOperatorArgs {
            level: Some(name.to_string()),
            sound: Some(true),
            ..args("Blocked", "")
        };

        // When it is read
        let notice = asked
            .into_notice()
            .unwrap_or_else(|e| panic!("{name}: {e}"));

        // Then the level and the sound are what was asked
        assert_eq!((notice.level, notice.sound), (level, true), "level {name}");
    }
}

#[test]
fn a_level_that_is_not_one_of_the_four_is_refused_naming_them() {
    // Given a level the tool does not know
    let asked = NotifyOperatorArgs {
        level: Some("urgent".to_string()),
        ..args("Blocked", "")
    };

    // When it is read
    let refusal = refusal_of(asked);

    // Then it is refused, naming the level and the four it may be
    for needle in ["urgent", "info", "success", "warning", "error"] {
        assert!(refusal.contains(needle), "{needle} in {refusal}");
    }
}

#[test]
fn a_title_empty_after_trimming_is_refused() {
    let refusal = refusal_of(args("   ", "a body"));

    assert!(refusal.contains("title is empty"), "{refusal}");
}

#[test]
fn a_title_of_exactly_the_limit_is_accepted_and_one_over_is_refused_never_truncated() {
    // Given titles at the limit and one character over
    let at_limit = "t".repeat(MAX_TITLE_CHARS);
    let over = "t".repeat(MAX_TITLE_CHARS + 1);

    // When each is read
    // Then the first is a notice with the whole title, and the second is refused with its length
    assert_eq!(
        args(&at_limit, "").into_notice().map(|notice| notice.title),
        Ok(at_limit)
    );
    let refusal = refusal_of(args(&over, ""));
    assert!(
        refusal.contains(&(MAX_TITLE_CHARS + 1).to_string()),
        "{refusal}"
    );
}

#[test]
fn a_body_of_exactly_the_limit_is_accepted_and_one_over_is_refused() {
    let at_limit = "b".repeat(MAX_BODY_CHARS);
    let over = "b".repeat(MAX_BODY_CHARS + 1);

    assert!(args("Done", &at_limit).into_notice().is_ok());
    let refusal = refusal_of(args("Done", &over));
    assert!(
        refusal.contains(&(MAX_BODY_CHARS + 1).to_string()),
        "{refusal}"
    );
}

#[test]
fn a_refusal_names_the_tool() {
    let refusal = refusal_of(args("", ""));

    assert!(refusal.starts_with(NOTIFY_OPERATOR), "{refusal}");
}
