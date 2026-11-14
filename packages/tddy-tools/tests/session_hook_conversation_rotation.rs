//! A claude-cli hook tells the daemon which conversation its agent is in, and the event's own
//! fields, so the daemon can follow the session through `/clear` (`#orchestrators` 6/11). The hook
//! matches nothing itself: whether a report is a rotation is the agent spec's
//! `conversation.rotation`, which the daemon reads.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-10-conversation-rotation.md`.

use pretty_assertions::assert_eq;
use tddy_service::proto::activity::ReportSessionStatusRequest;
use tddy_tools::session_hook::{session_status_request, SessionHookArgs};

const THE_SESSION: &str = "01900000-0000-7000-8000-0000000006a1";
const THE_POST_CLEAR_CONVERSATION: &str = "5b0c1d2e-0000-4000-8000-00000000c1ea";

fn the_hook() -> SessionHookArgs {
    SessionHookArgs {
        session: THE_SESSION.to_string(),
        daemon: "http://127.0.0.1:8899".to_string(),
        daemon_socket: None,
        os_user: "dev".to_string(),
        hook_token: "token-1".to_string(),
        event: "SessionStart".to_string(),
        await_answer: false,
        await_timeout_secs: tddy_tools::session_hook::DEFAULT_AWAIT_TIMEOUT_SECS,
    }
}

/// What Claude Code's `SessionStart` hook receives on stdin right after `/clear`.
fn a_session_start_after_a_clear() -> String {
    serde_json::json!({
        "hook_event_name": "SessionStart",
        "session_id": THE_POST_CLEAR_CONVERSATION,
        "transcript_path": format!("/home/dev/.claude/projects/-work/{THE_POST_CLEAR_CONVERSATION}.jsonl"),
        "cwd": "/work",
        "source": "clear",
    })
    .to_string()
}

/// The event's field `name`, as the report carries it.
fn the_field<'a>(request: &'a ReportSessionStatusRequest, name: &str) -> Option<&'a str> {
    request.hook_fields.get(name).map(String::as_str)
}

#[test]
fn a_session_start_hook_with_source_clear_forwards_the_new_conversation_id() {
    // Given Claude Code starting a fresh conversation after `/clear`
    let stdin = a_session_start_after_a_clear();

    // When
    let request = session_status_request(&the_hook(), &stdin).expect("a status report");

    // Then the report names the event, the conversation the agent is now in, and the event's
    // fields a spec's rotation can match on
    assert_eq!(request.status, "Started");
    assert_eq!(request.hook_event_name, "SessionStart");
    assert_eq!(request.agent_conversation_id, THE_POST_CLEAR_CONVERSATION);
    assert_eq!(the_field(&request, "source"), Some("clear"));
    assert_eq!(
        the_field(&request, "session_id"),
        Some(THE_POST_CLEAR_CONVERSATION)
    );
    // And it is still reported for the daemon's session, not the agent's conversation
    assert_eq!(request.session_id, THE_SESSION);
}

#[test]
fn every_status_report_names_the_conversation_its_event_came_from() {
    // Given Claude Code stopping after a turn in the post-clear conversation
    let stdin = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": THE_POST_CLEAR_CONVERSATION,
    })
    .to_string();

    // When
    let request = session_status_request(&the_hook(), &stdin).expect("a status report");

    // Then
    assert_eq!(request.agent_conversation_id, THE_POST_CLEAR_CONVERSATION);
}
