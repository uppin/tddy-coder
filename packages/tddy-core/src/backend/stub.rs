//! Stub backend for demo and workflow testing.
//!
//! Stateful backend that uses magic catch-words in the prompt to determine behavior.
//! Responses include help content explaining each step. Deterministic outputs for
//! each goal enable workflow tests to assert on exact state transitions.

use super::{
    ClarificationQuestion, CodingBackend, Goal, InvokeRequest, InvokeResponse, QuestionOption,
};
use crate::error::BackendError;
use std::sync::atomic::{AtomicU32, Ordering};

const STRUCTURED_OPEN: &str = "<structured-response content-type=\"application-json\">";
const STRUCTURED_CLOSE: &str = "</structured-response>";

fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Magic catch-words in the prompt (uppercased before check).
const SKIP_QUESTIONS: &str = "SKIP_QUESTIONS";
const FAIL_PARSE: &str = "FAIL_PARSE";
const FAIL_SCHEMA: &str = "FAIL_SCHEMA";
const FAIL_INVOKE: &str = "FAIL_INVOKE";
const SLOW: &str = "SLOW";

/// Stub backend for demo and workflow tests.
///
/// All interactions (clarification, permission, demo approval) rise from the stub:
/// - Plan: always asks clarification; when answered, proceeds.
/// - AcceptanceTests: always asks permission (like Claude) before creating files.
/// - Plan includes demo_plan so demo approval is requested after green.
#[derive(Debug)]
pub struct StubBackend {
    invocation_count: AtomicU32,
}

impl Default for StubBackend {
    fn default() -> Self {
        Self {
            invocation_count: AtomicU32::new(0),
        }
    }
}

impl StubBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn wrap_structured(json: &str) -> String {
        format!(
            "{}\n\n**[STUB]** {}\n\n{}{}{}",
            "Stub backend response.",
            "This is help content explaining the step.",
            STRUCTURED_OPEN,
            json,
            STRUCTURED_CLOSE
        )
    }

    fn plan_response(&self) -> InvokeResponse {
        // Omit discovery when not demo_mode (null fails for type:object). demo_plan only in demo_mode.
        // PRD includes TDD flow instructions so the demo covers the entire flow.
        let prd = r#"# PRD — Stub Feature

## Summary

Stub-generated PRD for tddy-demo. This demonstrates the TDD workflow.

## Next Steps — Full TDD Flow

After planning, run each step (or omit `--goal` to run the full flow interactively):

1. **Acceptance tests**: `tddy-demo --goal acceptance-tests --plan-dir <plan_dir>`
2. **Red** (failing tests): `tddy-demo --goal red --plan-dir <plan_dir>`
3. **Green** (implement): `tddy-demo --goal green --plan-dir <plan_dir>`
4. **Demo** (verify): `tddy-demo --goal demo --plan-dir <plan_dir>`
5. **Evaluate** (risk analysis): `tddy-demo --goal evaluate --plan-dir <plan_dir>`
6. **Validate** (tests, prod-ready, clean-code): `tddy-demo --goal validate --plan-dir <plan_dir>`
7. **Refactor** (apply refactoring plan): `tddy-demo --goal refactor --plan-dir <plan_dir>`

Or run `tddy-demo` with no `--goal` to continue the full workflow from the TUI."#;
        let todo = "- [ ] Run acceptance-tests\n- [ ] Run red\n- [ ] Run green\n- [ ] Run demo\n- [ ] Run evaluate\n- [ ] Run validate\n- [ ] Run refactor";
        let demo_plan = r#","demo_plan":{"demo_type":"cli","setup_instructions":"Run cargo build","steps":[{"description":"Run the CLI","command_or_action":"cargo run","expected_result":"See output"}],"verification":"CLI runs without error"}"#;
        let json = format!(
            r#"{{"goal":"plan","name":"Stub Feature","prd":"{}","todo":"{}"{}}}"#,
            escape_json_string(prd),
            escape_json_string(todo),
            demo_plan
        );
        InvokeResponse {
            output: Self::wrap_structured(&json),
            exit_code: 0,
            session_id: Some(format!(
                "stub-{}",
                self.invocation_count.load(Ordering::SeqCst)
            )),
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn acceptance_tests_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"acceptance-tests","summary":"Created 2 stub tests.","tests":[{"name":"test_auth","file":"tests/auth.it.rs","line":10,"status":"failing"},{"name":"test_logout","file":"tests/auth.it.rs","line":25,"status":"failing"}],"test_command":"cargo test","prerequisite_actions":"None"}"#;
        InvokeResponse {
            output: Self::wrap_structured(json),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn red_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"red","summary":"Created skeletons and failing tests.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}]}"#;
        InvokeResponse {
            output: Self::wrap_structured(json),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn green_response(&self) -> InvokeResponse {
        // Omit "reason" (null fails schema validation for type:string).
        let json = r#"{"goal":"green","summary":"Implemented and tests pass.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"passing"}],"implementations":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}]}"#;
        InvokeResponse {
            output: Self::wrap_structured(json),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn demo_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"demo","summary":"Demo completed.","demo_type":"cli","steps_completed":2,"verification":"All steps passed."}"#;
        InvokeResponse {
            output: Self::wrap_structured(json),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn evaluate_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"evaluate-changes","summary":"Evaluation complete.","risk_level":"low","changed_files":[],"affected_tests":[]}"#;
        InvokeResponse {
            output: Self::wrap_structured(json),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn validate_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"validate","summary":"Validation complete.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;
        InvokeResponse {
            output: Self::wrap_structured(json),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn refactor_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"refactor","summary":"Refactoring complete.","tasks_completed":1,"tests_passing":true}"#;
        InvokeResponse {
            output: Self::wrap_structured(json),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn response_for_goal(&self, goal: Goal) -> InvokeResponse {
        match goal {
            Goal::Plan => self.plan_response(),
            Goal::AcceptanceTests => self.acceptance_tests_response(),
            Goal::Red => self.red_response(),
            Goal::Green => self.green_response(),
            Goal::Demo => self.demo_response(),
            Goal::Evaluate => self.evaluate_response(),
            Goal::Validate => self.validate_response(),
            Goal::Refactor => self.refactor_response(),
        }
    }

    fn clarify_questions() -> Vec<ClarificationQuestion> {
        vec![ClarificationQuestion {
            header: "Scope".to_string(),
            question: "Which authentication method do you want?".to_string(),
            options: vec![
                QuestionOption {
                    label: "Email/password".to_string(),
                    description: "Traditional login".to_string(),
                },
                QuestionOption {
                    label: "OAuth".to_string(),
                    description: "Social login".to_string(),
                },
            ],
            multi_select: false,
            allow_other: true,
        }]
    }

    /// Permission question for acceptance-tests (demo mode): like Claude would ask before creating files.
    /// Binary Yes/No — no "Other (type your own)".
    fn permission_questions() -> Vec<ClarificationQuestion> {
        vec![ClarificationQuestion {
            header: "Permission".to_string(),
            question: "Allow creating test files (tests/auth.it.rs)?".to_string(),
            options: vec![
                QuestionOption {
                    label: "Yes".to_string(),
                    description: "Proceed with creating test files".to_string(),
                },
                QuestionOption {
                    label: "No".to_string(),
                    description: "Skip creating test files".to_string(),
                },
            ],
            multi_select: false,
            allow_other: false,
        }]
    }

    fn fail_parse_response(&self, _goal: Goal) -> InvokeResponse {
        // Malformed: no valid JSON, or wrong structure
        let garbage = "not valid json at all {{{";
        InvokeResponse {
            output: format!("{}{}{}", STRUCTURED_OPEN, garbage, STRUCTURED_CLOSE),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn fail_schema_response(&self, _goal: Goal) -> InvokeResponse {
        // Valid JSON but wrong goal or missing required fields
        let json = r#"{"goal":"wrong-goal","summary":"oops"}"#;
        InvokeResponse {
            output: Self::wrap_structured(json),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }
}

#[async_trait::async_trait]
impl CodingBackend for StubBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        self.invocation_count.fetch_add(1, Ordering::SeqCst);
        let prompt = request.prompt.to_uppercase();
        let has_answers = prompt.contains("HERE ARE THE USER'S ANSWERS");

        if prompt.contains(FAIL_INVOKE) {
            return Err(BackendError::InvocationFailed(
                "StubBackend: FAIL_INVOKE".to_string(),
            ));
        }

        if prompt.contains(FAIL_PARSE) {
            return Ok(self.fail_parse_response(request.goal));
        }

        if prompt.contains(FAIL_SCHEMA) {
            return Ok(self.fail_schema_response(request.goal));
        }

        // Plan: always clarify; when answered (HERE ARE THE USER'S ANSWERS), proceed.
        // SKIP_QUESTIONS: for tests (e.g. FlowRunner) that cannot provide clarification input.
        if request.goal == Goal::Plan && !has_answers && !prompt.contains(SKIP_QUESTIONS) {
            let mut resp = self.response_for_goal(request.goal);
            resp.questions = Self::clarify_questions();
            return Ok(resp);
        }

        // AcceptanceTests: always ask permission (like Claude) before creating files.
        if request.goal == Goal::AcceptanceTests && !has_answers && !prompt.contains(SKIP_QUESTIONS)
        {
            let mut resp = self.response_for_goal(request.goal);
            resp.questions = Self::permission_questions();
            return Ok(resp);
        }

        if prompt.contains(SLOW) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        Ok(self.response_for_goal(request.goal))
    }

    fn name(&self) -> &str {
        "stub"
    }
}
