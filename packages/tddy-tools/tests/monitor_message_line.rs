//! `tddy-tools monitor` prints a streamed event of kind `message` as a `message` line — the
//! message's id, sender, recipient and whole text, read from the event's log record — and every
//! other event as the `event` line #934 prints (`#activity-wake` 5/5).
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_tools::monitor_cli::{message_line_of, MonitorLine};

const CHILD: &str = "01900000-0000-7000-8000-AABB00000901";
const MAIN: &str = "01900000-0000-7000-8000-AABB00000900";

/// A streamed event as `WaitSessionWatchEvents` answers it: the child's message to the main
/// session, matched by watch `w-1`.
fn a_message_event(text: &str, reply_to: Option<&str>) -> Value {
    json!({
        "seq": 5_121,
        "watch_ids": ["w-1"],
        "kind": "message",
        "session_id": CHILD,
        "session": "fix-login",
        "summary": text,
        "record": {
            "seq": 5_121,
            "session_id": CHILD,
            "source": "message",
            "text": text,
            "message": {
                "to_session_id": MAIN,
                "message_id": "m-7c1d",
                "reply_to": reply_to,
                "text": text,
            },
        },
    })
}

#[test]
fn a_message_event_becomes_one_message_line() {
    // Given a streamed message replying to an earlier one, its text on two lines
    let event = a_message_event("Tests pass.\nOpen the PR?", Some("m-6b02"));

    // When the monitor turns it into its line
    let line = message_line_of(&event).map(|line| line.to_line());

    // Then it is one line carrying the message whole, its sender, recipient and what it replies to
    assert_eq!(
        line.as_deref()
            .map(|line| serde_json::from_str::<Value>(line).expect("the line is JSON")),
        Some(json!({
            "type": "message",
            "seq": 5_121,
            "watch_ids": ["w-1"],
            "message_id": "m-7c1d",
            "from_session_id": CHILD,
            "from": "fix-login",
            "to_session_id": MAIN,
            "reply_to": "m-6b02",
            "text": "Tests pass.\nOpen the PR?",
        }))
    );
    assert_eq!(line.map(|line| line.lines().count()), Some(1));
}

#[test]
fn an_event_of_another_kind_is_no_message_line() {
    // Given a streamed permission prompt
    let prompt = json!({
        "seq": 4_182,
        "watch_ids": ["w-1"],
        "kind": "permission",
        "session_id": CHILD,
        "session": "fix-login",
        "summary": "wants to run Bash: cargo test",
        "request_id": "req-1",
        "record": { "seq": 4_182, "session_id": CHILD, "source": "pending_question" },
    });

    // When the monitor asks for its message line
    let line: Option<MonitorLine> = message_line_of(&prompt);

    // Then there is none: it is printed as an event line
    assert_eq!(line, None);
}
