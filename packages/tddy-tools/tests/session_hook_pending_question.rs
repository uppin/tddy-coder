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
        await_answer: false,
        await_timeout_secs: tddy_tools::session_hook::DEFAULT_AWAIT_TIMEOUT_SECS,
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

// ---------------------------------------------------------------------------
// What the agent asks: `PermissionRequest` and `PreToolUse` of `AskUserQuestion`
// (`#home-inbox` 3/8)
// ---------------------------------------------------------------------------

/// Parse `json` — the tool input is compared as a value, not as the text the hook happened to
/// serialize, since the order of an object's keys is not part of the contract.
fn json_of(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("the hook sent JSON")
}

#[test]
fn a_permission_request_reports_waiting_with_the_tool_and_its_input() {
    // Given Claude Code's permission request for `cargo publish`
    let stdin = r#"{
        "hook_event_name": "PermissionRequest",
        "session_id": "claude-session",
        "tool_name": "Bash",
        "tool_input": { "command": "cargo publish", "description": "Publish the crate" }
    }"#;

    // When
    let request = session_status_request(&the_hook(), stdin).expect("a status report");

    // Then it reports waiting, naming the event, the tool and its input
    assert_eq!(request.status, "WaitingForInput");
    assert_eq!(request.hook_event_name, "PermissionRequest");
    assert_eq!(request.tool_name, "Bash");
    assert_eq!(
        json_of(&request.tool_input_json),
        serde_json::json!({ "command": "cargo publish", "description": "Publish the crate" })
    );
    // And it says nothing in the agent's words: the Notification hook that follows does
    assert_eq!(request.message, "");
}

#[test]
fn an_ask_user_question_reports_waiting_with_every_question_as_the_tool_input() {
    // Given Claude Code about to call AskUserQuestion
    let input = serde_json::json!({
        "questions": [{
            "question": "Which language should the service use?",
            "header": "Language",
            "multiSelect": false,
            "options": [
                { "label": "Rust", "description": "Memory safe, fast" },
                { "label": "Go", "description": "Simple to deploy" }
            ]
        }]
    });
    let stdin = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "AskUserQuestion",
        "tool_input": input,
    })
    .to_string();

    // When the hook that waits for the answer reports it
    let waiting = SessionHookArgs {
        await_answer: true,
        ..the_hook()
    };
    let request = session_status_request(&waiting, &stdin).expect("a status report");

    // Then it reports waiting, naming the event, the tool and the whole input
    assert_eq!(request.status, "WaitingForInput");
    assert_eq!(request.hook_event_name, "PreToolUse");
    assert_eq!(request.tool_name, "AskUserQuestion");
    assert_eq!(json_of(&request.tool_input_json), input);
}

// ---------------------------------------------------------------------------
// The catch-all `PreToolUse` hook reports the tool call, never a status (`#target-parity` 4/7)
// ---------------------------------------------------------------------------

#[test]
fn a_pre_tool_use_that_does_not_wait_reports_no_status() {
    // Given the catch-all PreToolUse hook (it does not wait), before a shell command and before a
    // question
    let before_bash = r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_use_id":"toolu_01","tool_input":{"command":"ls"}}"#;
    let before_a_question = r#"{"hook_event_name":"PreToolUse","tool_name":"AskUserQuestion","tool_input":{"questions":[]}}"#;

    // When
    let reports = (
        session_status_request(&the_hook(), before_bash),
        session_status_request(&the_hook(), before_a_question),
    );

    // Then neither reports a status: the call is reported as agent activity, and the question by
    // the hook that waits for its answer
    assert!(
        reports.0.is_none() && reports.1.is_none(),
        "a reporting PreToolUse sent a status: {reports:?}"
    );
}
