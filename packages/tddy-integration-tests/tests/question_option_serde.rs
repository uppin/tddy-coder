//! `QuestionOption` JSON: `description` may be omitted (agents often send label-only options).

use tddy_core::{ClarificationQuestion, QuestionOption};

#[test]
fn clarification_question_deserializes_options_without_description() {
    let json = r#"{
        "header": "H",
        "question": "Q?",
        "options": [{"label": "A"}],
        "multiSelect": false
    }"#;
    let q: ClarificationQuestion = serde_json::from_str(json).expect("deserialize");
    assert_eq!(q.options.len(), 1);
    assert_eq!(q.options[0].label, "A");
    assert_eq!(q.options[0].description, "");
}

#[test]
fn question_option_round_trips_with_empty_description() {
    let opt = QuestionOption {
        label: "x".to_string(),
        description: String::new(),
    };
    let v = serde_json::to_value(&opt).unwrap();
    let back: QuestionOption = serde_json::from_value(v).unwrap();
    assert_eq!(back.label, "x");
    assert_eq!(back.description, "");
}
