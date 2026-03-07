//! Acceptance tests step prompt and system prompt construction.

pub fn system_prompt() -> String {
    r#"You are an acceptance test creation assistant. Read the Testing Plan section from the PRD and create acceptance tests.

You MUST:
1. Create each acceptance test fully implemented (not stubs) - tests must fail because production code is missing
2. Run the project's test command to verify all new tests fail (Red state). Use the appropriate command for the project: `cargo test` for Rust, `npm test` or `npx jest` for Node/TypeScript, `pytest` for Python, etc.
3. Delete or adjust any tests that pass - passing tests do not verify new behavior
4. Do NOT ask for permission to write files - you have write access. Create the test files directly.
5. ALWAYS end your response with a structured-response block — REQUIRED even when summarizing existing work or when tests were already created in a previous turn.

Your final output MUST include this exact block (replace placeholders with actual values):

<structured-response content-type="application-json">
{"goal": "acceptance-tests", "summary": "<human-readable summary>", "tests": [{"name": "<test_name>", "file": "<path>", "line": <number>, "status": "failing"}]}
</structured-response>

The summary must describe what tests exist and confirm all are failing. The tests array must list each acceptance test with name, file, line, and status."#
        .to_string()
}

pub fn build_prompt(prd_content: &str) -> String {
    format!(
        "Create acceptance tests based on the Testing Plan in this PRD:\n\n{}",
        prd_content
    )
}

/// Build the follow-up prompt when the user has answered clarification questions.
pub fn build_followup_prompt(prd_content: &str, answers: &str) -> String {
    format!(
        r#"Here are the user's answers to your questions:

{answers}

Now create the acceptance tests based on the Testing Plan in this PRD:

{prd}"#,
        answers = answers.trim(),
        prd = prd_content
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_includes_instructions_for_test_creation_and_verification() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("Testing Plan") || prompt.contains("acceptance test"),
            "system prompt must instruct Claude to create tests and verify they fail"
        );
    }
}
