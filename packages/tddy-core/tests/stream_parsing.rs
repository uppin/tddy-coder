//! Integration tests for NDJSON stream parsing.
//!
//! Verifies that the stream processor extracts result text, session_id,
//! and AskUserQuestion tool events from Claude's stream-json output.

use std::io::Cursor;
use tddy_core::output::parse_planning_response;
use tddy_core::stream::cursor::process_cursor_stream;
use tddy_core::stream::{
    parse_clarification_questions_from_text, process_ndjson_stream, ProgressEvent,
};

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
    let result =
        process_ndjson_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(result.session_id, "sess-123");
    assert!(result.result_text.contains("---PRD_START---"));
    assert!(result.result_text.contains("Feature X"));
    assert!(result.result_text.contains("---TODO_START---"));
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

/// NDJSON where structured output is in user tool_result (Agent tool return) instead of result event.
/// Fallback for Claude Code CLI empty result bug (issue #7124).
const NDJSON_STRUCTURED_IN_TOOL_RESULT: &str = r#"{"type":"system","subtype":"init","session_id":"s1"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"Create acceptance tests"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"<structured-response content-type=\"application-json\">{\"goal\":\"acceptance-tests\",\"summary\":\"Created 2 tests.\",\"tests\":[{\"name\":\"test_a\",\"file\":\"tests/a.rs\",\"line\":1,\"status\":\"failing\"}]}</structured-response>"}]}}
{"type":"result","subtype":"success","result":"","session_id":"s1","is_error":false}
"#;

#[test]
fn process_ndjson_extracts_structured_response_from_tool_result() {
    let cursor = Cursor::new(NDJSON_STRUCTURED_IN_TOOL_RESULT);
    let result =
        process_ndjson_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(result.session_id, "s1");
    assert!(
        result.result_text.contains("<structured-response"),
        "should extract from user tool_result when result event is empty"
    );
    assert!(result.result_text.contains("acceptance-tests"));
    assert!(result.result_text.contains("Created 2 tests"));
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

/// Cursor emits assistant events with partial text chunks. Structured-response may be split across
/// multiple events. We must concatenate chunks and parse the combined result.
fn cursor_partial_chunks_ndjson() -> String {
    let chunk1 = r#"{"type":"system","subtype":"init","session_id":"cursor-sess"}"#;
    let chunk2 = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"<structured-response content-type=\"application-json\">"}]}}"#;
    // JSON string value must use escaped \n. Use r### so "##" in prd doesn't end the raw string.
    let plan_json = r###"{"goal":"plan","prd":"# X\n\n## TODO\n\n- [ ] T1"}"###;
    let chunk3 = format!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{}"}}]}}}}"#,
        plan_json.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let chunk4 = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"</structured-response>"}]}}"#;
    let chunk5 = r#"{"type":"result","subtype":"success","result":"","session_id":"cursor-sess","is_error":false}"#;
    format!(
        "{}\n{}\n{}\n{}\n{}\n",
        chunk1, chunk2, chunk3, chunk4, chunk5
    )
}

#[test]
fn process_cursor_stream_concatenates_partial_chunks_for_parsing() {
    let ndjson = cursor_partial_chunks_ndjson();
    let cursor = Cursor::new(ndjson);
    let result =
        process_cursor_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(result.session_id, "cursor-sess");
    assert!(
        result.result_text.contains("<structured-response"),
        "concatenated text should contain structured-response, got: {}",
        result.result_text
    );
    assert!(
        result.result_text.contains("</structured-response>"),
        "concatenated text should contain closing tag, got: {}",
        result.result_text
    );

    let planning =
        parse_planning_response(&result.result_text).expect("should parse concatenated chunks");
    assert!(planning.prd.contains("# X"));
    assert!(planning.prd.contains("T1"));
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

/// parse_clarification_questions_from_text extracts questions from structured XML block in agent output.
#[test]
fn parse_clarification_questions_from_text_extracts_questions() {
    let text = r#"I need more information before creating the plan.

<clarification-questions content-type="application-json">
{"questions":[{"header":"Tech Stack","question":"Which tech stack?","options":[{"label":"React","description":"React with hooks"},{"label":"Vanilla","description":"Vanilla JS"}],"multiSelect":false},{"header":"Scope","question":"What is the target audience?","options":[],"multiSelect":false}]}
</clarification-questions>"#;
    let questions = parse_clarification_questions_from_text(text);
    assert_eq!(questions.len(), 2);
    assert_eq!(questions[0].header, "Tech Stack");
    assert_eq!(questions[0].question, "Which tech stack?");
    assert_eq!(questions[0].options.len(), 2);
    assert_eq!(questions[0].options[0].label, "React");
    assert!(!questions[0].multi_select);
    assert_eq!(questions[1].header, "Scope");
    assert_eq!(questions[1].question, "What is the target audience?");
    assert!(questions[1].options.is_empty());
}

/// parse_clarification_questions_from_text returns empty when block is absent.
#[test]
fn parse_clarification_questions_from_text_returns_empty_when_no_block() {
    let text = "Just some plain text without any structured block.";
    let questions = parse_clarification_questions_from_text(text);
    assert!(questions.is_empty());
}

/// Cursor stream falls back to text parsing when no AskQuestion tool events.
#[test]
fn process_cursor_stream_falls_back_to_clarification_questions_block() {
    let line1 = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"I need clarification.\n\n"}]}}"#;
    let line2 = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"<clarification-questions content-type=\"application-json\">{\"questions\":[{\"header\":\"Confirm\",\"question\":\"Proceed?\",\"options\":[{\"label\":\"Yes\",\"description\":\"OK\"}],\"multiSelect\":false}]}</clarification-questions>"}]}}"#;
    let line3 = r#"{"type":"result","result":"","session_id":"s3"}"#;
    let ndjson = format!("{}\n{}\n{}\n", line1, line2, line3);
    let cursor = Cursor::new(ndjson);
    let result =
        process_cursor_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(
        result.questions.len(),
        1,
        "should extract from text block when no tool events"
    );
    assert_eq!(result.questions[0].header, "Confirm");
    assert_eq!(result.questions[0].question, "Proceed?");
    assert_eq!(result.questions[0].options[0].label, "Yes");
}

/// Cursor stream with real-world tddy-coder plan output: streaming chunks + full assistant
/// message + result event with full content. Reproduces format from refactoring.txt (lines 986-988).
/// The result event contains the full concatenated output including the clarification block.
#[test]
fn process_cursor_stream_extracts_clarification_from_result_event_with_full_content() {
    let line1 =
        r#"{"type":"system","subtype":"init","session_id":"ee4e2182-c133-4a06-b51d-0c3db571bbc3"}"#;
    let line2 = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I'll start by exploring."}]},"session_id":"ee4e2182-c133-4a06-b51d-0c3db571bbc3"}"#;
    let line3 = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"\n</clarification-questions>"}]},"session_id":"ee4e2182-c133-4a06-b51d-0c3db571bbc3"}"#;
    let line4 = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Now I have comprehensive understanding. Before I produce the PRD, I have a few clarification points.\n\n<clarification-questions content-type=\"application-json\">\n{\"questions\":[{\"header\":\"Validate goal scope\",\"question\":\"When you say the new validate goal should produce refactoring-plan.md: should validate keep existing validate-refactor behavior AND add synthesis? Or replace both?\",\"options\":[{\"label\":\"Replace validate-refactor only\",\"description\":\"validate = renamed validate-refactor.\"},{\"label\":\"Merge both into one goal\",\"description\":\"validate replaces both.\"}],\"multiSelect\":false},{\"header\":\"Refactor goal behavior\",\"question\":\"For the new refactor goal: TDD cycle or direct execution?\",\"options\":[{\"label\":\"TDD cycle\",\"description\":\"mini red→green for each item\"},{\"label\":\"Direct execution\",\"description\":\"directly apply changes\"},{\"label\":\"Plan mode only\",\"description\":\"user manually applies\"}],\"multiSelect\":false},{\"header\":\"Full workflow chain\",\"question\":\"Where should validate and refactor fit?\",\"options\":[{\"label\":\"After evaluate\",\"description\":\"plan → ... → evaluate → validate → refactor\"},{\"label\":\"Standalone only\",\"description\":\"standalone goals only\"},{\"label\":\"Replace evaluate\",\"description\":\"validate replaces evaluate\"}],\"multiSelect\":false}]}\n</clarification-questions>"}]},"session_id":"ee4e2182-c133-4a06-b51d-0c3db571bbc3"}"#;
    let result_content = "I'll start by exploring.Now I have comprehensive understanding. Before I produce the PRD, I have a few clarification points.\n\n<clarification-questions content-type=\"application-json\">\n{\"questions\":[{\"header\":\"Validate goal scope\",\"question\":\"When you say the new validate goal should produce refactoring-plan.md: should validate keep existing validate-refactor behavior AND add synthesis? Or replace both?\",\"options\":[{\"label\":\"Replace validate-refactor only\",\"description\":\"validate = renamed validate-refactor.\"},{\"label\":\"Merge both into one goal\",\"description\":\"validate replaces both.\"}],\"multiSelect\":false},{\"header\":\"Refactor goal behavior\",\"question\":\"For the new refactor goal: TDD cycle or direct execution?\",\"options\":[{\"label\":\"TDD cycle\",\"description\":\"mini red→green for each item\"},{\"label\":\"Direct execution\",\"description\":\"directly apply changes\"},{\"label\":\"Plan mode only\",\"description\":\"user manually applies\"}],\"multiSelect\":false},{\"header\":\"Full workflow chain\",\"question\":\"Where should validate and refactor fit?\",\"options\":[{\"label\":\"After evaluate\",\"description\":\"plan → ... → evaluate → validate → refactor\"},{\"label\":\"Standalone only\",\"description\":\"standalone goals only\"},{\"label\":\"Replace evaluate\",\"description\":\"validate replaces evaluate\"}],\"multiSelect\":false}]}\n</clarification-questions>";
    let line5 = format!(
        r#"{{"type":"result","subtype":"success","duration_ms":121950,"is_error":false,"result":{},"session_id":"ee4e2182-c133-4a06-b51d-0c3db571bbc3"}}"#,
        serde_json::to_string(result_content).expect("escape result")
    );
    let ndjson = format!("{}\n{}\n{}\n{}\n{}\n", line1, line2, line3, line4, line5);
    let cursor = Cursor::new(ndjson);
    let result =
        process_cursor_stream(cursor, |_| {}, |_| {}, None, None, 0).expect("should process");

    assert_eq!(
        result.session_id, "ee4e2182-c133-4a06-b51d-0c3db571bbc3",
        "should capture session_id from result event"
    );
    assert_eq!(
        result.questions.len(),
        3,
        "should extract 3 clarification questions from result event with full content; got: {:?}",
        result
            .questions
            .iter()
            .map(|q| &q.header)
            .collect::<Vec<_>>()
    );
    assert_eq!(result.questions[0].header, "Validate goal scope");
    assert_eq!(result.questions[1].header, "Refactor goal behavior");
    assert_eq!(result.questions[2].header, "Full workflow chain");
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
