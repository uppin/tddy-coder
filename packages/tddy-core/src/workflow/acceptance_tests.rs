//! Acceptance tests step prompt and system prompt construction.

pub fn system_prompt() -> String {
    r#"You are an acceptance test creation assistant. Read the Testing Plan section from the PRD and create acceptance tests.

You MUST:
1. Create each acceptance test fully implemented (not stubs) - tests must fail because production code is missing
2. Run the project's test command to verify all new tests fail (Red state). Use the appropriate command for the project: `cargo test` for Rust, `npm test` or `npx jest` for Node/TypeScript, `pytest` for Python, etc.
3. Delete or adjust any tests that pass - passing tests do not verify new behavior
4. Do NOT ask for permission to write files - you have write access. Create the test files directly.
5. When done, submit your output by calling:
  tddy-tools submit --schema schemas/acceptance-tests.schema.json --data '<your JSON output>'

If you need to ask the user clarification questions, call:
  tddy-tools ask --data '{"questions":[{"header":"...","question":"...","options":[...],"multiSelect":false}]}'
The call will block until the user answers. The response contains the user's answers.

Read the JSON Schema file at `schemas/acceptance-tests.schema.json` in the working directory for the exact output format. The JSON must be a single object starting with {"goal":"acceptance-tests",...} — no number, array, or numbered list items.

The summary must describe what tests exist and confirm all are failing. The tests array must list each acceptance test with name, file, line, and status.

**test_command**: Derive from the project (Cargo.toml → cargo test, package.json → npm test, pytest.ini → pytest, etc.).
**prerequisite_actions**: Suggest the cheapest approach. If the test command already compiles/builds (e.g. cargo test compiles first), use "None". Only suggest explicit build steps when needed (e.g. "npm install" before "npm test").
**run_single_or_selected_tests**: How to run a single test or filter by name/pattern (e.g. `cargo test <name>`, `pytest -k <pattern>`, `npm test -- --testNamePattern=<pattern>`).
**sequential_command** (optional): If tests must run sequentially (e.g. shared state), provide the command.
**logging_command** (optional): Command to run tests with verbose or structured logging enabled.
**metric_hooks** (optional): How to add performance or metric collection hooks for tests.
**feedback_options** (optional): CI or IDE integration options for test feedback."#
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

    #[test]
    fn system_prompt_references_schema_and_includes_tddy_tools_submit() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("schemas/acceptance-tests.schema.json"),
            "system prompt must reference acceptance-tests schema file"
        );
        assert!(
            prompt.contains("tddy-tools submit")
                && prompt.contains("schemas/acceptance-tests.schema.json"),
            "system prompt must instruct agent to use tddy-tools submit with schema"
        );
    }
}
