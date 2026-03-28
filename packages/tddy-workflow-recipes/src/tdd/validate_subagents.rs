//! Validate-subagents goal prompt and system prompt construction.
//!
//! Orchestrates validate-tests, validate-prod-ready, and analyze-clean-code subagents via the Agent tool.

/// Return the system prompt for the validate goal (subagent-based).
pub fn system_prompt() -> String {
    r#"You are a refactor validation orchestrator. Using the Agent tool, spawn 3 concurrent subagents to analyze the codebase:

1. **validate-tests subagent**: Run the test suite, report which tests pass/fail, identify missing coverage.
2. **validate-prod-ready subagent**: Check production readiness: error handling, logging, configuration, security, performance. Include whether any Red-phase TDD JSON logging markers (`"tddy"` + `marker_id` style emissions) remain in production sources — they should be absent after **refactor**.
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
5. When done, submit your output by calling:
  tddy-tools submit --goal validate --data '<your JSON output>'

If you need to ask the user clarification questions, call:
  tddy-tools ask --data '{"questions":[{"header":"...","question":"...","options":[...],"multiSelect":false}]}'
The call will block until the user answers. The response contains the user's answers.

Run `tddy-tools get-schema validate` to see the expected output format. The JSON must be a single object starting with {"goal":"validate",...}."#
    .to_string()
}

/// Build the user-facing prompt for the validate goal (subagent-based).
///
/// - `evaluation_report_content`: content of evaluation-report.md from session_dir
///
/// The prompt instructs the agent to orchestrate the 3 validation subagents.
pub fn build_prompt(evaluation_report_content: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_references_schema_and_includes_tddy_tools_submit() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("tddy-tools get-schema validate"),
            "system prompt must reference get-schema for validate"
        );
        assert!(
            prompt.contains("tddy-tools submit") && prompt.contains("--goal validate"),
            "system prompt must instruct agent to use tddy-tools submit --goal validate"
        );
    }

    #[test]
    fn validate_prod_ready_checks_red_markers_absent() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("marker_id") && prompt.contains("refactor"),
            "validate-prod-ready subagent brief must mention Red markers vs refactor"
        );
    }
}
