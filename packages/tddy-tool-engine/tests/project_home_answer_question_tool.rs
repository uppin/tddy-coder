//! `AnswerSessionQuestion`'s definition and arguments (`#home-inbox` 8/9). Its dispatch runs
//! through `ProjectHomeAnswerPort::answer_question`, which `question_answer_acceptance.rs` and
//! `project_home_answer_question_acceptance.rs` exercise end to end.

use serde_json::json;
use tddy_service::proto::activity::QuestionAnswer;
use tddy_tool_engine::project_home_answer_tools::{
    is_project_home_answer_tool, project_home_answer_tool_defs, AnswerQuestionArgs,
    ANSWER_SESSION_PERMISSION, ANSWER_SESSION_QUESTION,
};

#[test]
fn both_answer_tools_are_answer_tools() {
    assert!(is_project_home_answer_tool(ANSWER_SESSION_PERMISSION));
    assert!(is_project_home_answer_tool(ANSWER_SESSION_QUESTION));
    assert!(!is_project_home_answer_tool("ReadSessionActivity"));
}

#[test]
fn the_definitions_list_the_permission_tool_then_the_question_tool() {
    // When
    let defs = project_home_answer_tool_defs();

    // Then
    let names: Vec<&str> = defs.iter().map(|def| def.name.as_str()).collect();
    assert_eq!(names, [ANSWER_SESSION_PERMISSION, ANSWER_SESSION_QUESTION]);
}

#[test]
fn the_question_tool_requires_its_session_request_and_answers_and_takes_the_target_host() {
    // When
    let defs = project_home_answer_tool_defs();
    let def = defs
        .iter()
        .find(|def| def.name == ANSWER_SESSION_QUESTION)
        .expect("the question tool is defined");

    // Then
    let schema: serde_json::Value =
        serde_json::from_str(&def.input_schema_json).expect("the schema is JSON");
    assert_eq!(
        schema["required"],
        json!(["session_id", "request_id", "answers"])
    );
    assert!(
        schema["properties"]["daemon_instance_id"].is_object(),
        "a target tool takes the host it runs on: {schema}"
    );
}

#[test]
fn the_arguments_parse_into_the_wire_answers_with_free_text_optional() {
    // Given
    let args = json!({
        "session_id": "child",
        "request_id": "req-1",
        "answers": [
            { "question_index": 0, "selected": ["Rust"] },
            { "question_index": 1, "other": "bench" }
        ]
    });

    // When
    let parsed: AnswerQuestionArgs = serde_json::from_value(args).expect("valid arguments");
    let answers: Vec<QuestionAnswer> = parsed.answers.into_iter().map(Into::into).collect();

    // Then
    assert_eq!(
        answers,
        vec![
            QuestionAnswer {
                question_index: 0,
                selected: vec!["Rust".to_string()],
                other: None
            },
            QuestionAnswer {
                question_index: 1,
                selected: Vec::new(),
                other: Some("bench".to_string())
            },
        ]
    );
}
