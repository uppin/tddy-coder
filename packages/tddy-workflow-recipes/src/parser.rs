//! Parser for LLM structured output.
//!
//! All structured output is received via `tddy-tools submit` (Unix socket IPC).
//! Parser functions accept pre-validated JSON strings and deserialize into typed structs.
//! Questions are extracted from AskUserQuestion tool events in the NDJSON stream, not from text.

use tddy_core::error::ParseError;

mod planning;
pub use planning::*;

mod acceptance_tests;
pub use acceptance_tests::*;

// ── analyze output (bugfix pipeline) ─────────────────────────────────────────

mod analyze;
pub use analyze::*;

mod green;
pub use green::*;

mod red;
pub use red::*;

mod evaluate;
pub use evaluate::*;

// ── validate (subagents) output types ─────────────────────────────────────────

/// Parsed output from the validate goal (subagent-based).
#[derive(Debug, Clone)]
pub struct ValidateSubagentsOutput {
    pub goal: String,
    pub summary: String,
    pub tests_report_written: bool,
    pub prod_ready_report_written: bool,
    pub clean_code_report_written: bool,
    pub refactoring_plan_written: bool,
    /// Markdown body for `refactoring-plan.md` when included in `tddy-tools submit` JSON.
    pub refactoring_plan: Option<String>,
}

#[derive(serde::Deserialize)]
struct StructuredValidateRefactor {
    goal: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    tests_report_written: Option<bool>,
    #[serde(default)]
    prod_ready_report_written: Option<bool>,
    #[serde(default)]
    clean_code_report_written: Option<bool>,
    #[serde(default)]
    refactoring_plan_written: Option<bool>,
    #[serde(default)]
    refactoring_plan: Option<String>,
}

/// Parse LLM validate (subagent) response. JSON must come from tddy-tools submit.
pub fn parse_validate_subagents_response(s: &str) -> Result<ValidateSubagentsOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredValidateRefactor = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("validate") {
        return Err(ParseError::Malformed(format!(
            "goal must be validate, got: {:?}",
            parsed.goal
        )));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| "No summary provided.".to_string());

    log::debug!(
        "[tddy-core] parse_validate_subagents_response: summary length={}, tests_written={:?}",
        summary.len(),
        parsed.tests_report_written
    );

    Ok(ValidateSubagentsOutput {
        goal: "validate".to_string(),
        summary,
        tests_report_written: parsed.tests_report_written.unwrap_or(false),
        prod_ready_report_written: parsed.prod_ready_report_written.unwrap_or(false),
        clean_code_report_written: parsed.clean_code_report_written.unwrap_or(false),
        refactoring_plan_written: parsed.refactoring_plan_written.unwrap_or(false),
        refactoring_plan: parsed.refactoring_plan.filter(|s| !s.trim().is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_planning_response_accepts_valid_json() {
        // Given
        let input = "{\"goal\":\"plan\",\"prd\":\"# PRD\\n\\n## Summary\\nFeature X\\n\\n## TODO\\n\\n- [ ] Task 1\"}";

        // When
        let out = planning::parse_planning_response(input).expect("should parse");

        // Then
        assert!(out.prd.contains("Feature X"));
        assert!(out.prd.contains("Task 1"));
    }

    #[test]
    fn parse_planning_response_rejects_non_json() {
        // When
        let err = planning::parse_planning_response("Some random text without JSON").unwrap_err();

        // Then
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn parse_planning_response_rejects_wrong_goal() {
        // When
        let err = planning::parse_planning_response(
            "{\"goal\":\"red\",\"prd\":\"# PRD\\n\\n## TODO\\n\\n- [ ] T1\"}",
        )
        .unwrap_err();

        // Then
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn parse_planning_response_rejects_empty_prd() {
        // When
        let err = planning::parse_planning_response(r#"{"goal":"plan","prd":"   "}"#).unwrap_err();

        // Then
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn converting_red_output_to_progress_markdown_produces_unfilled_checkboxes() {
        use super::red::{RedTestInfo, SkeletonInfo};

        // Given
        let out = red::RedOutput {
            summary: "Created skeletons.".into(),
            tests: vec![
                RedTestInfo {
                    name: "test_foo".into(),
                    file: "src/foo.rs".into(),
                    line: Some(10),
                    status: "failing".into(),
                },
                RedTestInfo {
                    name: "test_bar".into(),
                    file: "src/bar.rs".into(),
                    line: None,
                    status: "failing".into(),
                },
            ],
            skeletons: vec![SkeletonInfo {
                name: "Foo".into(),
                file: "src/foo.rs".into(),
                line: Some(5),
                kind: "struct".into(),
            }],
            test_command: None,
            prerequisite_actions: None,
            run_single_or_selected_tests: None,
            markers: vec![],
            marker_results: vec![],
            test_output_file: None,
            sequential_command: None,
            logging_command: None,
            metric_hooks: None,
            feedback_options: None,
        };

        // When
        let md = out.to_progress_markdown();

        // Then
        assert!(md.contains("## Failed Tests"));
        assert!(md.contains("## Skeletons"));
        assert!(md.contains("- [ ] test_foo (src/foo.rs:10)"));
        assert!(md.contains("- [ ] test_bar (src/bar.rs)"));
        assert!(md.contains("- [ ] Foo (src/foo.rs:5) — struct"));
    }

    #[test]
    fn parse_red_response_extracts_summary_tests_skeletons() {
        // Given
        let input = r#"{"goal":"red","summary":"Created 2 skeletons and 1 failing test.","tests":[{"name":"test_foo","file":"src/foo.rs","line":10,"status":"failing"}],"skeletons":[{"name":"Foo","file":"src/foo.rs","line":5,"kind":"struct"},{"name":"bar","file":"src/foo.rs","line":8,"kind":"method"}]}"#;

        // When
        let out = super::red::parse_red_response(input).expect("should parse");

        // Then
        assert!(out.summary.contains("2 skeletons"));
        assert_eq!(out.tests.len(), 1);
        assert_eq!(out.tests[0].name, "test_foo");
        assert_eq!(out.tests[0].file, "src/foo.rs");
        assert_eq!(out.tests[0].line, Some(10));
        assert_eq!(out.tests[0].status, "failing");
        assert_eq!(out.skeletons.len(), 2);
        assert_eq!(out.skeletons[0].name, "Foo");
        assert_eq!(out.skeletons[0].kind, "struct");
        assert_eq!(out.skeletons[1].name, "bar");
        assert_eq!(out.skeletons[1].kind, "method");
    }

    #[test]
    fn parse_red_response_extracts_test_command_and_prerequisite_actions() {
        // Given
        let input = r#"{"goal":"red","summary":"Created skeletons.","tests":[],"skeletons":[],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

        // When
        let out = super::red::parse_red_response(input).expect("should parse");

        // Then
        assert_eq!(out.test_command.as_deref(), Some("cargo test"));
        assert_eq!(out.prerequisite_actions.as_deref(), Some("None"));
        assert_eq!(
            out.run_single_or_selected_tests.as_deref(),
            Some("cargo test <name>")
        );
    }

    #[test]
    fn validate_red_marker_source_paths_accepts_production_only_markers() {
        // Given
        let out = red::RedOutput {
            summary: "s".into(),
            tests: vec![],
            skeletons: vec![],
            test_command: None,
            prerequisite_actions: None,
            run_single_or_selected_tests: None,
            markers: vec![MarkerInfo {
                marker_id: "M001".into(),
                test_name: "t".into(),
                scope: "scope".into(),
                data: serde_json::json!({}),
                source_file: Some("packages/demo/src/widget.rs".into()),
            }],
            marker_results: vec![],
            test_output_file: None,
            sequential_command: None,
            logging_command: None,
            metric_hooks: None,
            feedback_options: None,
        };

        // When / Then — must not error
        red::validate_red_marker_source_paths(&out)
            .expect("production-only markers should validate");
    }

    #[test]
    fn parse_acceptance_tests_response_extracts_summary_and_tests() {
        // Given
        let input = r#"{"goal":"acceptance-tests","summary":"Created 2 acceptance tests. All failing (Red state) as expected.","tests":[{"name":"login_stores_session_token","file":"packages/auth/tests/session.it.rs","line":15,"status":"failing"},{"name":"logout_clears_session","file":"packages/auth/tests/session.it.rs","line":28,"status":"failing"}]}"#;

        // When
        let out = acceptance_tests::parse_acceptance_tests_response(input).expect("should parse");

        // Then
        assert!(out.summary.contains("Created 2 acceptance tests"));
        assert_eq!(out.tests.len(), 2);
        assert_eq!(out.tests[0].name, "login_stores_session_token");
        assert_eq!(out.tests[0].file, "packages/auth/tests/session.it.rs");
        assert_eq!(out.tests[0].line, Some(15));
        assert_eq!(out.tests[0].status, "failing");
    }

    #[test]
    fn parse_acceptance_tests_response_extracts_test_command_and_prerequisite_actions() {
        // Given
        let input = r#"{"goal":"acceptance-tests","summary":"Created 2 tests.","tests":[{"name":"t1","file":"t.rs","line":1,"status":"failing"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

        // When
        let out =
            super::acceptance_tests::parse_acceptance_tests_response(input).expect("should parse");

        // Then
        assert_eq!(out.test_command.as_deref(), Some("cargo test"));
        assert_eq!(out.prerequisite_actions.as_deref(), Some("None"));
        assert_eq!(
            out.run_single_or_selected_tests.as_deref(),
            Some("cargo test <name>")
        );
    }

    #[test]
    fn parse_green_response_extracts_summary_tests_implementations() {
        // Given
        let input = r#"{"goal":"green","summary":"Implemented 2 methods. All tests passing.","tests":[{"name":"test_foo","file":"src/foo.rs","line":10,"status":"passing"},{"name":"test_bar","file":"src/bar.rs","line":20,"status":"failing","reason":"timeout"}],"implementations":[{"name":"AuthService::validate","file":"src/service.rs","line":15,"kind":"method"}]}"#;

        // When
        let out = green::parse_green_response(input).expect("should parse");

        // Then
        assert!(out.summary.contains("All tests passing"));
        assert_eq!(out.tests.len(), 2);
        assert_eq!(out.tests[0].name, "test_foo");
        assert_eq!(out.tests[0].status, "passing");
        assert_eq!(out.tests[1].status, "failing");
        assert_eq!(out.tests[1].reason.as_deref(), Some("timeout"));
        assert_eq!(out.implementations.len(), 1);
        assert_eq!(out.implementations[0].name, "AuthService::validate");
        assert_eq!(out.implementations[0].kind, "method");
    }

    #[test]
    fn parse_green_response_extracts_test_command_fields() {
        // Given
        let input = r#"{"goal":"green","summary":"Implemented.","tests":[],"implementations":[],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

        // When
        let out = green::parse_green_response(input).expect("should parse");

        // Then
        assert_eq!(out.test_command.as_deref(), Some("cargo test"));
        assert_eq!(out.prerequisite_actions.as_deref(), Some("None"));
        assert_eq!(
            out.run_single_or_selected_tests.as_deref(),
            Some("cargo test <name>")
        );
    }

    #[test]
    fn parse_green_response_errors_on_wrong_goal() {
        // When
        let err = green::parse_green_response(
            r#"{"goal":"red","summary":"Wrong goal.","tests":[],"implementations":[]}"#,
        )
        .unwrap_err();

        // Then
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn converting_green_output_to_progress_markdown_marks_passing_and_failing() {
        use super::green::{GreenTestResult, ImplementationInfo};

        // Given
        let out = green::GreenOutput {
            summary: "Implemented.".into(),
            tests: vec![
                GreenTestResult {
                    name: "test_foo".into(),
                    file: "src/foo.rs".into(),
                    line: Some(10),
                    status: "passing".into(),
                    reason: None,
                },
                GreenTestResult {
                    name: "test_bar".into(),
                    file: "src/bar.rs".into(),
                    line: Some(20),
                    status: "failing".into(),
                    reason: Some("timeout".into()),
                },
            ],
            implementations: vec![ImplementationInfo {
                name: "Foo".into(),
                file: "src/foo.rs".into(),
                line: Some(5),
                kind: "struct".into(),
            }],
            test_command: None,
            prerequisite_actions: None,
            run_single_or_selected_tests: None,
            demo_results: None,
        };

        // When
        let md = out.to_updated_progress_markdown();

        // Then
        assert!(md.contains("- [x] test_foo"));
        assert!(md.contains("- [!] test_bar"));
        assert!(md.contains("timeout"));
        assert!(md.contains("- [x] Foo"));
    }
}

/// Parse the standalone demo goal. JSON must come from tddy-tools submit.
pub fn parse_demo_response(s: &str) -> Result<green::DemoOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredDemo = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("demo") {
        return Err(ParseError::Malformed(format!(
            "goal is not demo, got: {:?}",
            parsed.goal
        )));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("summary missing or empty".into()))?;

    log::debug!(
        "[tddy-core] parse_demo_response: summary_len={}, steps={}",
        summary.len(),
        parsed.steps_completed.unwrap_or(0)
    );

    Ok(green::DemoOutput {
        summary,
        demo_type: parsed.demo_type.unwrap_or_else(|| "unknown".to_string()),
        steps_completed: parsed.steps_completed.unwrap_or(0),
        verification: parsed.verification.unwrap_or_default(),
        share_url: parsed.share_url,
    })
}

// ── refactor output types ────────────────────────────────────────────────────

/// Parsed output from the refactor goal.
#[derive(Debug, Clone)]
pub struct RefactorOutput {
    pub summary: String,
    pub tasks_completed: u32,
    pub tests_passing: bool,
}

#[derive(serde::Deserialize)]
struct StructuredRefactor {
    goal: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    tasks_completed: Option<u32>,
    #[serde(default)]
    tests_passing: Option<bool>,
}

/// Parse LLM refactor response. JSON must come from tddy-tools submit.
pub fn parse_refactor_response(s: &str) -> Result<RefactorOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredRefactor = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("refactor") {
        return Err(ParseError::Malformed(format!(
            "goal is not refactor, got: {:?}",
            parsed.goal
        )));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("summary missing or empty".into()))?;
    Ok(RefactorOutput {
        summary,
        tasks_completed: parsed.tasks_completed.unwrap_or(0),
        tests_passing: parsed.tests_passing.unwrap_or(false),
    })
}

// ── update-docs output types ─────────────────────────────────────────────────

/// Parsed output from the update-docs goal.
#[derive(Debug, Clone)]
pub struct UpdateDocsOutput {
    pub summary: String,
    pub docs_updated: u32,
}

#[derive(serde::Deserialize)]
struct StructuredUpdateDocs {
    goal: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    docs_updated: Option<u32>,
}

/// Parse LLM update-docs response. JSON must come from tddy-tools submit.
pub fn parse_update_docs_response(s: &str) -> Result<UpdateDocsOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredUpdateDocs = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("update-docs") {
        return Err(ParseError::Malformed(format!(
            "goal is not update-docs, got: {:?}",
            parsed.goal
        )));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("summary missing or empty".into()))?;
    Ok(UpdateDocsOutput {
        summary,
        docs_updated: parsed.docs_updated.unwrap_or(0),
    })
}

#[derive(serde::Deserialize)]
struct StructuredDemo {
    goal: Option<String>,
    summary: Option<String>,
    demo_type: Option<String>,
    steps_completed: Option<u32>,
    verification: Option<String>,
    share_url: Option<String>,
}

#[cfg(test)]
mod exploration_artifact_tests {
    use super::*;

    #[test]
    fn plan_response_retains_the_exploration_field_through_parsing() {
        // Given
        let json = r##"{"goal":"plan","prd":"# PRD\n## TODO\n- [ ] t","exploration":"# Exploration\n\n## Code Map\n\n- `src/lib.rs:10:1` — entry point"}"##;

        // When
        let parsed = planning::parse_planning_response(json).expect("plan response should parse");

        // Then — serialize back: the exploration knowledge must survive the round-trip
        let value = serde_json::to_value(&parsed).expect("serialize planning output");
        assert_eq!(
            value.get("exploration").and_then(|v| v.as_str()),
            Some("# Exploration\n\n## Code Map\n\n- `src/lib.rs:10:1` — entry point"),
            "PlanningOutput must retain the plan submit exploration field"
        );
    }
}
