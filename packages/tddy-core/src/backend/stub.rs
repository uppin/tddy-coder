//! Stub backend for demo and workflow testing.
//!
//! Stateful backend that uses magic catch-words in the prompt to determine behavior.
//! Submits structured results via ToolExecutor (in-memory for tests, process for tddy-demo).
//! Deterministic outputs for each goal enable workflow tests to assert on exact state transitions.

use super::{
    ClarificationQuestion, CodingBackend, Goal, InvokeRequest, InvokeResponse, QuestionOption,
    ToolExecutor,
};
use crate::error::BackendError;
use crate::stream::ProgressEvent;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

fn emit_agent_exited(request: &InvokeRequest, exit_code: i32) {
    if let Some(ref sink) = request.progress_sink {
        sink.emit(&ProgressEvent::AgentExited {
            exit_code,
            goal: request.goal.submit_key().to_string(),
        });
    }
}

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
///
/// Uses ToolExecutor to submit results: tests inject InMemoryToolExecutor,
/// tddy-demo injects ProcessToolExecutor (tddy-tools submit).
pub struct StubBackend {
    invocation_count: AtomicU32,
    tool_executor: Arc<dyn ToolExecutor>,
    submit_channel: Option<crate::toolcall::SubmitResultChannel>,
}

impl StubBackend {
    pub fn new() -> Self {
        let executor = super::InMemoryToolExecutor::new();
        let channel = executor.channel().clone();
        Self {
            invocation_count: AtomicU32::new(0),
            tool_executor: Arc::new(executor),
            submit_channel: Some(channel),
        }
    }

    pub fn with_executor(tool_executor: Arc<dyn ToolExecutor>) -> Self {
        Self {
            invocation_count: AtomicU32::new(0),
            tool_executor,
            submit_channel: None,
        }
    }

    fn submit_and_respond(
        &self,
        goal: &str,
        json: &str,
        session_id: Option<String>,
    ) -> InvokeResponse {
        log::debug!(
            "[stub] submitting goal={} via tool_executor (json len={})",
            goal,
            json.len()
        );
        if let Err(e) = self.tool_executor.submit(goal, json) {
            log::warn!(
                "[stub] tool_executor.submit failed for goal {}: {}",
                goal,
                e
            );
        }
        InvokeResponse {
            output:
                "Stub backend response.\n\n**[STUB]** This is help content explaining the step."
                    .to_string(),
            exit_code: 0,
            session_id,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn plan_response(&self) -> InvokeResponse {
        // Omit discovery when not demo_mode (null fails for type:object). demo_plan only in demo_mode.
        // PRD includes TDD flow instructions and ## TODO section (single PRD format).
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

Or run `tddy-demo` with no `--goal` to continue the full workflow from the TUI.

## TODO

- [ ] Create auth module
- [ ] Implement login endpoint
- [ ] Implement logout endpoint"#;
        let demo_plan = r#","demo_plan":{"demo_type":"cli","setup_instructions":"Run cargo build","steps":[{"description":"Run the CLI","command_or_action":"cargo run","expected_result":"See output"}],"verification":"CLI runs without error"}"#;
        let json = format!(
            r#"{{"goal":"plan","name":"Stub Feature","prd":"{}"{}}}"#,
            escape_json_string(prd),
            demo_plan
        );
        self.submit_and_respond(
            "plan",
            &json,
            Some(format!(
                "stub-{}",
                self.invocation_count.load(Ordering::SeqCst)
            )),
        )
    }

    fn acceptance_tests_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"acceptance-tests","summary":"Created 2 stub tests.","tests":[{"name":"test_auth","file":"tests/auth.it.rs","line":10,"status":"failing"},{"name":"test_logout","file":"tests/auth.it.rs","line":25,"status":"failing"}],"test_command":"cargo test","prerequisite_actions":"None"}"#;
        self.submit_and_respond("acceptance-tests", json, None)
    }

    fn red_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"red","summary":"Created skeletons and failing tests.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}]}"#;
        self.submit_and_respond("red", json, None)
    }

    fn green_response(&self) -> InvokeResponse {
        // Omit "reason" (null fails schema validation for type:string).
        let json = r#"{"goal":"green","summary":"Implemented and tests pass.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"passing"}],"implementations":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}]}"#;
        self.submit_and_respond("green", json, None)
    }

    fn demo_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"demo","summary":"Demo completed.","demo_type":"cli","steps_completed":2,"verification":"All steps passed."}"#;
        self.submit_and_respond("demo", json, None)
    }

    fn evaluate_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"evaluate-changes","summary":"Evaluation complete.","risk_level":"low","changed_files":[],"affected_tests":[]}"#;
        self.submit_and_respond("evaluate-changes", json, None)
    }

    fn validate_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"validate","summary":"Validation complete.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;
        self.submit_and_respond("validate", json, None)
    }

    fn refactor_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"refactor","summary":"Refactoring complete.","tasks_completed":1,"tests_passing":true}"#;
        self.submit_and_respond("refactor", json, None)
    }

    fn update_docs_response(&self) -> InvokeResponse {
        let json = r#"{"goal":"update-docs","summary":"Documentation updated.","docs_updated":3}"#;
        self.submit_and_respond("update-docs", json, None)
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
            Goal::UpdateDocs => self.update_docs_response(),
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
        // Malformed: no valid JSON, or wrong structure. No tool call.
        let garbage = "not valid json at all {{{";
        InvokeResponse {
            output: garbage.to_string(),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }

    fn fail_schema_response(&self, _goal: Goal) -> InvokeResponse {
        // Valid JSON but wrong goal or missing required fields. No tool call.
        let json = r#"{"goal":"wrong-goal","summary":"oops"}"#;
        InvokeResponse {
            output: json.to_string(),
            exit_code: 0,
            session_id: None,
            questions: vec![],
            raw_stream: None,
            stderr: None,
        }
    }
}

impl std::fmt::Debug for StubBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StubBackend")
            .field(
                "invocation_count",
                &self.invocation_count.load(Ordering::SeqCst),
            )
            .finish()
    }
}

impl Default for StubBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CodingBackend for StubBackend {
    fn submit_channel(&self) -> Option<&crate::toolcall::SubmitResultChannel> {
        self.submit_channel.as_ref()
    }

    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let n = self.invocation_count.fetch_add(1, Ordering::SeqCst) + 1;
        log::debug!("[stub] invoke #{} goal={:?}", n, request.goal);
        let prompt = request.prompt.to_uppercase();
        let has_answers = prompt.contains("HERE ARE THE USER'S ANSWERS");

        if prompt.contains(FAIL_INVOKE) {
            return Err(BackendError::InvocationFailed(
                "StubBackend: FAIL_INVOKE".to_string(),
            ));
        }

        if prompt.contains(FAIL_PARSE) {
            emit_agent_exited(&request, 0);
            return Ok(self.fail_parse_response(request.goal));
        }

        if prompt.contains(FAIL_SCHEMA) {
            emit_agent_exited(&request, 0);
            return Ok(self.fail_schema_response(request.goal));
        }

        // Plan: always clarify; when answered (HERE ARE THE USER'S ANSWERS) or refinement, proceed.
        // SKIP_QUESTIONS: for tests (e.g. FlowRunner) that cannot provide clarification input.
        // Refinement: "The user has reviewed the plan and requested refinements" — skip questions.
        // (prompt is uppercased above)
        // CRITICAL: Do NOT call response_for_goal when returning questions — that would submit
        // the plan prematurely. Submit only when we have answers and are returning the final plan.
        let is_refinement =
            prompt.contains("THE USER HAS REVIEWED THE PLAN AND REQUESTED REFINEMENTS");
        if request.goal == Goal::Plan
            && !has_answers
            && !prompt.contains(SKIP_QUESTIONS)
            && !is_refinement
        {
            log::debug!("[stub] plan: returning clarification questions (no submit)");
            emit_agent_exited(&request, 0);
            return Ok(InvokeResponse {
                output: "Clarification needed.".to_string(),
                exit_code: 0,
                session_id: Some(format!(
                    "stub-{}",
                    self.invocation_count.load(Ordering::SeqCst)
                )),
                questions: Self::clarify_questions(),
                raw_stream: None,
                stderr: None,
            });
        }

        // AcceptanceTests: always ask permission (like Claude) before creating files.
        // Same as Plan: do NOT submit when returning questions.
        if request.goal == Goal::AcceptanceTests && !has_answers && !prompt.contains(SKIP_QUESTIONS)
        {
            log::debug!("[stub] acceptance-tests: returning permission questions (no submit)");
            emit_agent_exited(&request, 0);
            return Ok(InvokeResponse {
                output: "Permission needed.".to_string(),
                exit_code: 0,
                session_id: None,
                questions: Self::permission_questions(),
                raw_stream: None,
                stderr: None,
            });
        }

        if prompt.contains(SLOW) {
            log::debug!("[stub] SLOW: sleeping 50ms");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        log::debug!("[stub] returning response_for_goal {:?}", request.goal);
        let response = self.response_for_goal(request.goal);
        emit_agent_exited(&request, response.exit_code);
        Ok(response)
    }

    fn name(&self) -> &str {
        "stub"
    }
}
