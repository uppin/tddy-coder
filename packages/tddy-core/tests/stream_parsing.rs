//! Integration tests for NDJSON stream parsing.
//!
//! Verifies that the stream processor extracts result text, session_id,
//! and AskUserQuestion tool events from Claude's stream-json output.

use std::io::Cursor;
use tddy_core::stream::cursor::process_cursor_stream;
use tddy_core::stream::{process_ndjson_stream, ProgressEvent};

/// Minimal NDJSON that produces result text and session_id in result event.
const NDJSON_WITH_RESULT: &str = r#"{"type":"system","subtype":"init","session_id":"sess-123"}
{"type":"assistant","message":{"content":[{"type":"text","text":"Analyzing..."}]}}
{"type":"result","subtype":"success","result":"Analysis complete. Feature X with Task 1.","session_id":"sess-123","is_error":false}
"#;

/// NDJSON with AskUserQuestion tool_use event.
const NDJSON_WITH_QUESTIONS: &str = r#"{"type":"system","subtype":"init","session_id":"sess-456"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"AskUserQuestion","input":{"questions":[{"question":"What is the target?","header":"Scope","options":[{"label":"A","description":"Option A"},{"label":"B","description":"Option B"}],"multiSelect":false}]}}]}}
{"type":"result","subtype":"success","result":"","session_id":"sess-456","is_error":false}
"#;

#[test]
fn process_ndjson_extracts_result_text_and_session_id() {
    let cursor = Cursor::new(NDJSON_WITH_RESULT);
    let result =
        process_ndjson_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(result.session_id, "sess-123");
    assert!(result.result_text.contains("Feature X"));
    assert!(result.result_text.contains("Task 1"));
    assert!(result.questions.is_empty());
}

#[test]
fn process_ndjson_extracts_ask_user_question_events() {
    let cursor = Cursor::new(NDJSON_WITH_QUESTIONS);
    let result =
        process_ndjson_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

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
    let result =
        process_ndjson_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(
        result.questions.len(),
        1,
        "duplicate questions should be deduplicated"
    );
    assert_eq!(result.questions[0].question, "Q1");
}

/// NDJSON with AskUserQuestion in permission_denials (tool was denied, questions in result event).
const NDJSON_PERMISSION_DENIALS: &str = r#"{"type":"system","subtype":"init","session_id":"sess-denied"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"AskUserQuestion","input":{"questions":[{"question":"Activation method?","header":"Activation","options":[{"label":"Just type","description":"Start typing"},{"label":"Press i","description":"Vim-style"}],"multiSelect":false}]}}]}}
{"type":"result","subtype":"success","result":"","session_id":"sess-denied","is_error":false,"permission_denials":[{"tool_name":"AskUserQuestion","tool_use_id":"t1","tool_input":{"questions":[{"question":"Activation method?","header":"Activation","options":[{"label":"Just type","description":"Start typing"},{"label":"Press i","description":"Vim-style"}],"multiSelect":false}]}}]}
"#;

#[test]
fn process_ndjson_extracts_questions_from_permission_denials() {
    let cursor = Cursor::new(NDJSON_PERMISSION_DENIALS);
    let result =
        process_ndjson_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(result.session_id, "sess-denied");
    assert_eq!(result.questions.len(), 1);
    assert_eq!(result.questions[0].header, "Activation");
    assert_eq!(result.questions[0].question, "Activation method?");
    assert_eq!(result.questions[0].options.len(), 2);
    assert_eq!(result.questions[0].options[0].label, "Just type");
}

/// Real-world result event from Claude Code (workflow-fixes.txt) with permission_denials.
/// Verifies parser handles extra fields (usage, modelUsage, etc.) and extracts questions.
#[test]
fn process_ndjson_extracts_questions_from_workflow_fixes_format() {
    let ndjson = r#"{"type":"system","subtype":"init","session_id":"43c6980b-00c2-4de7-8bc5-5901bc3d85eb"}
{"type":"result","subtype":"success","is_error":false,"duration_ms":103957,"result":"","stop_reason":"end_turn","session_id":"43c6980b-00c2-4de7-8bc5-5901bc3d85eb","total_cost_usd":0.579661,"usage":{"input_tokens":1217},"modelUsage":{"claude-opus-4-6":{"inputTokens":1217}},"permission_denials":[{"tool_name":"AskUserQuestion","tool_use_id":"t1","tool_input":{"questions":[{"question":"In point #2, what are the two goal names?","header":"Point #2","options":[{"label":"demo after green","description":"A separate demo goal after green"},{"label":"validate after green","description":"A separate validate goal after green"}],"multiSelect":false},{"question":"In point #4, what should validate-changes be renamed to?","header":"Point #4","options":[{"label":"validate","description":"Rename to validate"},{"label":"review","description":"Rename to review"}],"multiSelect":false}]}}]}"#;
    let cursor = Cursor::new(ndjson);
    let result =
        process_ndjson_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(result.session_id, "43c6980b-00c2-4de7-8bc5-5901bc3d85eb");
    assert_eq!(
        result.questions.len(),
        2,
        "should extract both questions from permission_denials"
    );
    assert_eq!(result.questions[0].header, "Point #2");
    assert_eq!(
        result.questions[0].question,
        "In point #2, what are the two goal names?"
    );
    assert_eq!(result.questions[1].header, "Point #4");
    assert_eq!(
        result.questions[1].question,
        "In point #4, what should validate-changes be renamed to?"
    );
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
    let result = process_ndjson_stream(cursor, |ev| events.push(ev.clone()), |_| {}, None, None, 0)
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
    process_ndjson_stream(cursor, |ev| events.push(ev.clone()), |_| {}, None, None, 0)
        .expect("should process");

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
    process_ndjson_stream(cursor, |ev| events.push(ev.clone()), |_| {}, None, None, 0)
        .expect("should process");

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
    process_ndjson_stream(cursor, |ev| events.push(ev.clone()), |_| {}, None, None, 0)
        .expect("should process");

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
    process_ndjson_stream(cursor, |ev| events.push(ev.clone()), |_| {}, None, None, 0)
        .expect("should process");

    assert_eq!(
        events.len(),
        2,
        "TaskStarted + TaskProgress only, no ToolUse from sub-agent"
    );
    assert!(matches!(&events[0], ProgressEvent::TaskStarted { .. }));
    assert!(matches!(&events[1], ProgressEvent::TaskProgress { .. }));
}

/// Cursor emits assistant events with partial text chunks. We must concatenate chunks.
fn cursor_partial_chunks_ndjson() -> String {
    let chunk1 = r#"{"type":"system","subtype":"init","session_id":"cursor-sess"}"#;
    let chunk2 = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Analyzing "}]}}"#;
    let chunk3 = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Feature X"}]}}"#;
    let chunk4 = r#"{"type":"assistant","message":{"content":[{"type":"text","text":" with Task 1."}]}}"#;
    let chunk5 = r#"{"type":"result","subtype":"success","result":"","session_id":"cursor-sess","is_error":false}"#;
    format!(
        "{}\n{}\n{}\n{}\n{}\n",
        chunk1, chunk2, chunk3, chunk4, chunk5
    )
}

#[test]
fn process_cursor_stream_concatenates_partial_chunks() {
    let ndjson = cursor_partial_chunks_ndjson();
    let cursor = Cursor::new(ndjson);
    let result =
        process_cursor_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(result.session_id, "cursor-sess");
    assert!(
        result.result_text.contains("Analyzing"),
        "concatenated text should contain first chunk, got: {}",
        result.result_text
    );
    assert!(
        result.result_text.contains("Feature X"),
        "concatenated text should contain second chunk, got: {}",
        result.result_text
    );
    assert!(
        result.result_text.contains("Task 1"),
        "concatenated text should contain third chunk, got: {}",
        result.result_text
    );
}

/// Cursor tool_call events should emit ToolUse with displayable detail (glob pattern, file path).
fn cursor_tool_calls_with_detail_ndjson() -> String {
    let line1 = r#"{"type":"system","subtype":"init","session_id":"s1"}"#;
    let line2 = r#"{"type":"tool_call","subtype":"started","tool_call":{"globToolCall":{"args":{"globPattern":"*.md"}}},"session_id":"s1"}"#;
    let line3 = r#"{"type":"tool_call","subtype":"started","tool_call":{"readToolCall":{"args":{"path":"/project/README.md"}}},"session_id":"s1"}"#;
    let line4 = r#"{"type":"result","subtype":"success","result":"done","session_id":"s1","is_error":false}"#;
    format!("{}\n{}\n{}\n{}\n", line1, line2, line3, line4)
}

#[test]
fn process_cursor_stream_emits_tool_use_with_detail_for_display() {
    let ndjson = cursor_tool_calls_with_detail_ndjson();
    let cursor = Cursor::new(ndjson);
    let mut events = Vec::new();
    let _ = process_cursor_stream(cursor, |ev| events.push(ev.clone()), |_| {}, None, None, 0)
        .expect("should process");

    assert_eq!(events.len(), 2, "should emit 2 ToolUse events");
    assert!(
        matches!(&events[0], ProgressEvent::ToolUse { name, detail: Some(d) } if name == "glob" && d == "*.md"),
        "glob should have detail *.md, got: {:?}",
        events[0]
    );
    assert!(
        matches!(&events[1], ProgressEvent::ToolUse { name, detail: Some(d) } if name == "read" && d == "README.md"),
        "read should have detail README.md (filename), got: {:?}",
        events[1]
    );
}

/// Cursor askUserQuestionToolCall should extract questions for Q&A flow.
/// Schema: {"type":"tool_call","subtype":"started","tool_call":{"askUserQuestionToolCall":{"args":{"questions":[...]}}}}
fn cursor_ask_question_ndjson() -> String {
    let line1 = r#"{"type":"system","subtype":"init","session_id":"s1"}"#;
    let line2 = r#"{"type":"tool_call","subtype":"started","call_id":"t1","tool_call":{"askUserQuestionToolCall":{"args":{"questions":[{"question":"Which tech stack?","header":"Tech Stack","options":[{"label":"React","description":"React with hooks"},{"label":"Vanilla","description":"Vanilla JS"}],"multiSelect":false}]}}},"session_id":"s1"}"#;
    let line3 = r#"{"type":"result","subtype":"success","result":"","session_id":"s1"}"#;
    format!("{}\n{}\n{}\n", line1, line2, line3)
}

#[test]
fn process_cursor_stream_extracts_ask_user_question_for_qa() {
    let ndjson = cursor_ask_question_ndjson();
    let cursor = Cursor::new(ndjson);
    let result =
        process_cursor_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(result.questions.len(), 1, "should extract 1 question");
    assert_eq!(result.questions[0].header, "Tech Stack");
    assert_eq!(result.questions[0].question, "Which tech stack?");
    assert_eq!(result.questions[0].options.len(), 2);
    assert_eq!(result.questions[0].options[0].label, "React");
    assert_eq!(
        result.questions[0].options[0].description,
        "React with hooks"
    );
    assert!(!result.questions[0].multi_select);
}

/// Cursor askQuestionToolCall (alternative name) should also extract questions.
#[test]
fn process_cursor_stream_extracts_ask_question_tool_call() {
    let line1 = r#"{"type":"system","subtype":"init","session_id":"s2"}"#;
    let line2 = r#"{"type":"tool_call","subtype":"started","tool_call":{"askQuestionToolCall":{"args":{"questions":[{"question":"Proceed?","header":"Confirm","options":[{"label":"Yes","description":"Continue"},{"label":"No","description":"Cancel"}],"multiSelect":false}]}}},"session_id":"s2"}"#;
    let line3 = r#"{"type":"result","result":"","session_id":"s2"}"#;
    let ndjson = format!("{}\n{}\n{}\n", line1, line2, line3);
    let cursor = Cursor::new(ndjson);
    let result =
        process_cursor_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(result.questions.len(), 1);
    assert_eq!(result.questions[0].header, "Confirm");
    assert_eq!(result.questions[0].question, "Proceed?");
    assert_eq!(result.questions[0].options[0].label, "Yes");
}
