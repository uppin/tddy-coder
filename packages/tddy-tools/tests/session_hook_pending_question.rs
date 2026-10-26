//! A claude-cli hook that reports `WaitingForInput` tells the daemon what the agent is waiting on
//! (`#start-parity` 6/9).

use tddy_tools::session_hook::{session_status_request, SessionHookArgs};

fn the_hook() -> SessionHookArgs {
    SessionHookArgs {
        session: "session-1".to_string(),
        daemon: "http://127.0.0.1:8899".to_string(),
        daemon_socket: None,
        os_user: "dev".to_string(),
        hook_token: "token-1".to_string(),
        event: String::new(),
    }
}

#[test]
fn a_permission_prompt_reports_waiting_with_the_agents_question() {
    // Given Claude Code's permission notification
    let stdin = r#"{
        "hook_event_name": "Notification",
        "notification_type": "permission_prompt",
        "message": "Claude needs your permission to use Bash"
    }"#;

    // When
    let request = session_status_request(&the_hook(), stdin).expect("a status report");

    // Then
    assert_eq!(request.status, "WaitingForInput");
    assert_eq!(request.message, "Claude needs your permission to use Bash");
    assert_eq!(request.session_id, "session-1");
}

#[test]
fn a_status_that_is_not_waiting_carries_no_message() {
    // Given the agent stopping, with a message of its own
    let stdin = r#"{ "hook_event_name": "Stop", "message": "done" }"#;

    // When
    let request = session_status_request(&the_hook(), stdin).expect("a status report");

    // Then
    assert_ne!(request.status, "WaitingForInput");
    assert_eq!(request.message, "");
}

#[test]
fn an_event_that_reports_no_status_sends_nothing() {
    // When
    let request = session_status_request(&the_hook(), r#"{ "hook_event_name": "PreCompact" }"#);

    // Then
    assert!(request.is_none());
}
