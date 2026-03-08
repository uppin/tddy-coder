//! Validate-refactor goal prompt and system prompt construction.

/// Return the system prompt for the validate-refactor goal.
pub fn system_prompt() -> String {
    r#"You are a refactor validation orchestrator. Using the Agent tool, spawn 3 concurrent subagents to analyze the codebase:

1. **validate-tests subagent**: Run the test suite, report which tests pass/fail, identify missing coverage.
2. **validate-prod-ready subagent**: Check production readiness: error handling, logging, configuration, security, performance.
3. **analyze-clean-code subagent**: Analyze code quality: naming, complexity, duplication, SOLID principles, documentation.

Do NOT use ExitPlanMode or EnterPlanMode.

Each subagent MUST write its findings to a Markdown report in the plan directory:
- validate-tests-report.md
- validate-prod-ready-report.md
- analyze-clean-code-report.md

You MUST:
1. Read evaluation-report.md from the plan directory for context
2. Spawn all 3 subagents concurrently using the Agent tool
3. Wait for all 3 to complete
4. Report whether each report was written
5. ALWAYS end your response with a structured-response block — REQUIRED.

**CRITICAL**: The content between <structured-response> and </structured-response> MUST be exactly one valid JSON object starting with {"goal":"validate-refactor",...}.

<structured-response content-type="application-json">
{"goal":"validate-refactor","summary":"<human-readable summary of all 3 subagent results>","tests_report_written":<true|false>,"prod_ready_report_written":<true|false>,"clean_code_report_written":<true|false>}
</structured-response>"#
    .to_string()
}

/// Build the user-facing prompt for validate-refactor.
///
/// - `evaluation_report_content`: content of evaluation-report.md from plan_dir
///
/// The prompt instructs the agent to orchestrate the 3 validation subagents.
pub fn build_prompt(evaluation_report_content: &str) -> String {
    eprintln!(
        r#"{{"tddy":{{"marker_id":"M008c","scope":"workflow::validate_refactor::build_prompt","data":{{}}}}}}"#
    );
    format!(
        r#"Orchestrate a full refactor validation. Use the Agent tool to spawn 3 concurrent validation subagents:

1. validate-tests: run the test suite and write validate-tests-report.md
2. validate-prod-ready: check production readiness and write validate-prod-ready-report.md
3. analyze-clean-code: analyze code quality and write analyze-clean-code-report.md

The evaluation report from the prior evaluate-changes run is provided below for context:

## Evaluation Report

{evaluation_report}

Spawn all 3 subagents concurrently. When all are done, report the summary and whether each report was written."#,
        evaluation_report = evaluation_report_content
    )
}
