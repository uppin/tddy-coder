//! Integration tests for NDJSON stream parsing.
//!
//! Verifies that the stream processor extracts result text, session_id,
//! and AskUserQuestion tool events from Claude's stream-json output.

use std::io::Cursor;
use tddy_core::stream::{process_ndjson_stream, ProgressEvent};

/// Minimal NDJSON that produces PRD+TODO in result event.
const NDJSON_WITH_RESULT: &str = r#"{"type":"system","subtype":"init","session_id":"sess-123"}
{"type":"assistant","message":{"content":[{"type":"text","text":"Analyzing..."}]}}
{"type":"result","subtype":"success","result":"---PRD_START---\n# PRD\nFeature X\n---PRD_END---\n---TODO_START---\n- [ ] Task 1\n---TODO_END---","session_id":"sess-123","is_error":false}
"#;

/// NDJSON with AskUserQuestion tool_use event.
const NDJSON_WITH_QUESTIONS: &str = r#"{"type":"system","subtype":"init","session_id":"sess-456"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"AskUserQuestion","input":{"questions":[{"question":"What is the target?","header":"Scope","options":[{"label":"A","description":"Option A"},{"label":"B","description":"Option B"}],"multiSelect":false}]}}]}}
{"type":"result","subtype":"success","result":"","session_id":"sess-456","is_error":false}
"#;

#[test]
fn process_ndjson_extracts_result_text_and_session_id() {
    let cursor = Cursor::new(NDJSON_WITH_RESULT);
    let result = process_ndjson_stream(cursor, |_| {}, |_| {}).expect("should process");

    assert_eq!(result.session_id, "sess-123");
    assert!(result.result_text.contains("---PRD_START---"));
    assert!(result.result_text.contains("Feature X"));
    assert!(result.result_text.contains("---TODO_START---"));
    assert!(result.questions.is_empty());
}

#[test]
fn process_ndjson_extracts_ask_user_question_events() {
    let cursor = Cursor::new(NDJSON_WITH_QUESTIONS);
    let result = process_ndjson_stream(cursor, |_| {}, |_| {}).expect("should process");

    assert_eq!(result.session_id, "sess-456");
    assert_eq!(result.questions.len(), 1);
    assert_eq!(result.questions[0].header, "Scope");
    assert_eq!(result.questions[0].question, "What is the target?");
    assert_eq!(result.questions[0].options.len(), 2);
    assert_eq!(result.questions[0].options[0].label, "A");
    assert_eq!(result.questions[0].options[0].description, "Option A");
    assert!(!result.questions[0].multi_select);
}

/// NDJSON with duplicate AskUserQuestion (same questions from multiple tool_use events).
const NDJSON_DUPLICATE_QUESTIONS: &str = r#"{"type":"system","subtype":"init","session_id":"sess-dup"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"AskUserQuestion","input":{"questions":[{"question":"Q1","header":"H1","options":[],"multiSelect":false}]}}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"AskUserQuestion","input":{"questions":[{"question":"Q1","header":"H1","options":[],"multiSelect":false}]}}]}}
{"type":"result","subtype":"success","result":"","session_id":"sess-dup","is_error":false}
"#;

#[test]
fn process_ndjson_deduplicates_questions() {
    let cursor = Cursor::new(NDJSON_DUPLICATE_QUESTIONS);
    let result = process_ndjson_stream(cursor, |_| {}, |_| {}).expect("should process");

    assert_eq!(
        result.questions.len(),
        1,
        "duplicate questions should be deduplicated"
    );
    assert_eq!(result.questions[0].question, "Q1");
}

/// NDJSON with task_started and task_progress system events.
const NDJSON_TASK_EVENTS: &str = r#"{"type":"system","subtype":"task_started","description":"Explore repo","task_id":"x"}
{"type":"system","subtype":"task_progress","description":"Running find...","last_tool_name":"Bash"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}
{"type":"result","subtype":"success","result":"done","session_id":"sess","is_error":false}
"#;

#[test]
fn process_ndjson_emits_progress_events_for_tasks_and_tools() {
    let cursor = Cursor::new(NDJSON_TASK_EVENTS);
    let mut events = Vec::new();
    let result = process_ndjson_stream(cursor, |ev| events.push(ev.clone()), |_| {})
        .expect("should process");

    assert_eq!(result.result_text, "done");
    assert_eq!(events.len(), 3);
    assert!(
        matches!(&events[0], ProgressEvent::TaskStarted { description } if description == "Explore repo")
    );
    assert!(
        matches!(&events[1], ProgressEvent::TaskProgress { description, last_tool: Some(t) } if description == "Running find..." && t == "Bash")
    );
    assert!(matches!(
        &events[2],
        ProgressEvent::ToolUse { name, detail } if name == "Read" && detail.is_none()
    ));
}

/// NDJSON with Glob tool_use that has pattern (detail extraction).
const NDJSON_GLOB_WITH_PATTERN: &str = r#"{"type":"system","subtype":"init","session_id":"s1"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Glob","input":{"pattern":"docs/**/*.md"}}]}}
{"type":"result","subtype":"success","result":"","session_id":"s1","is_error":false}
"#;

#[test]
fn process_ndjson_extracts_glob_pattern_as_detail() {
    let cursor = Cursor::new(NDJSON_GLOB_WITH_PATTERN);
    let mut events = Vec::new();
    process_ndjson_stream(cursor, |ev| events.push(ev.clone()), |_| {}).expect("should process");

    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        ProgressEvent::ToolUse { name, detail: Some(d) } if name == "Glob" && d == "docs/**/*.md"
    ));
}

/// NDJSON with Write tool_use that has file_path (detail extraction).
const NDJSON_WRITE_WITH_PATH: &str = r#"{"type":"system","subtype":"init","session_id":"s1"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Write","input":{"file_path":"docs/ft/PRD.md","content":"body"}}]}}
{"type":"result","subtype":"success","result":"","session_id":"s1","is_error":false}
"#;

#[test]
fn process_ndjson_extracts_write_file_path_as_detail() {
    let cursor = Cursor::new(NDJSON_WRITE_WITH_PATH);
    let mut events = Vec::new();
    process_ndjson_stream(cursor, |ev| events.push(ev.clone()), |_| {}).expect("should process");

    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        ProgressEvent::ToolUse { name, detail: Some(d) } if name == "Write" && d == "PRD.md"
    ));
}

/// NDJSON with Read tool_use that has file_path (detail extraction).
const NDJSON_READ_WITH_PATH: &str = r#"{"type":"system","subtype":"init","session_id":"s1"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"packages/tddy-core/src/lib.rs"}}]}}
{"type":"result","subtype":"success","result":"","session_id":"s1","is_error":false}
"#;

#[test]
fn process_ndjson_extracts_tool_use_detail_from_input() {
    let cursor = Cursor::new(NDJSON_READ_WITH_PATH);
    let mut events = Vec::new();
    process_ndjson_stream(cursor, |ev| events.push(ev.clone()), |_| {}).expect("should process");

    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        ProgressEvent::ToolUse { name, detail: Some(d) } if name == "Read" && d == "lib.rs"
    ));
}

/// Sub-agent assistant events (parent_tool_use_id set) should NOT emit ToolUse.
const NDJSON_SUBAGENT_TOOL_USE: &str = r#"{"type":"system","subtype":"init","session_id":"s1"}
{"type":"system","subtype":"task_started","description":"Explore repo","task_id":"x"}
{"type":"system","subtype":"task_progress","description":"Reading lib.rs","last_tool_name":"Read"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"packages/tddy-core/src/lib.rs"}}]},"parent_tool_use_id":"agent-123"}
{"type":"result","subtype":"success","result":"","session_id":"s1","is_error":false}
"#;

#[test]
fn process_ndjson_skips_tool_use_when_parent_tool_use_id_set() {
    let cursor = Cursor::new(NDJSON_SUBAGENT_TOOL_USE);
    let mut events = Vec::new();
    process_ndjson_stream(cursor, |ev| events.push(ev.clone()), |_| {}).expect("should process");

    assert_eq!(
        events.len(),
        2,
        "TaskStarted + TaskProgress only, no ToolUse from sub-agent"
    );
    assert!(matches!(&events[0], ProgressEvent::TaskStarted { .. }));
    assert!(matches!(&events[1], ProgressEvent::TaskProgress { .. }));
}
