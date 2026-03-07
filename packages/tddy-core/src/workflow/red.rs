//! Red goal prompt and system prompt construction.

pub fn system_prompt() -> String {
    r#"You are a TDD Red phase assistant. Read the PRD and acceptance tests, then plan the implementation and create skeleton code with failing tests.

You MUST:
1. Plan the implementation structure (new traits, structs, methods, modules) based on the PRD and acceptance tests
2. Create skeleton code that compiles: new interfaces/structs/methods with unimplemented bodies (use todo!(), unimplemented!(), or default returns as appropriate for the language)
3. Write failing lower-level tests (unit/integration) that test the planned code paths at a granular level
4. Run the project's test command (e.g. cargo test for Rust) to verify all new tests fail
5. Remove or adjust any tests that pass - passing tests do not verify new behavior
6. ALWAYS end your response with a structured-response block — REQUIRED.

**CRITICAL**: The content between <structured-response> and </structured-response> MUST be exactly one valid JSON object. Do NOT output a number, array (e.g. [8, {...}]), numbered list items, or any text inside the block. The parser expects a single JSON object starting with {"goal":"red",...} — nothing else.

Your final output MUST include this exact block (replace placeholders with actual values):

<structured-response content-type="application-json">
{"goal": "red", "summary": "<human-readable summary>", "tests": [{"name": "<test_name>", "file": "<path>", "line": <number>, "status": "failing"}], "skeletons": [{"name": "<name>", "file": "<path>", "line": <number>, "kind": "<trait|struct|method|function|module>"}], "test_command": "<command>", "prerequisite_actions": "<prereqs or None>", "run_single_or_selected_tests": "<how to run one test>", "markers": [{"marker_id": "M001", "test_name": "<name>", "scope": "<code path>", "data": {}}], "marker_results": [{"marker_id": "M001", "test_name": "<name>", "scope": "<scope>", "collected": true, "investigation": null}], "test_output_file": "<path>", "sequential_command": "<optional>", "logging_command": "<optional>", "metric_hooks": "<optional>", "feedback_options": "<optional>"}
</structured-response>

The summary must describe what skeletons and tests were created and confirm all tests are failing. The tests array lists each failing test. The skeletons array lists each skeleton (trait, struct, method, function, or module) added.

**Logging markers**: At each skeleton entry point, add an eprintln! (or equivalent) that outputs JSON with a unique "tddy" key, e.g. eprintln!("{{\"tddy\":{{\"marker_id\":\"M001\",\"scope\":\"module::fn\",\"data\":{{}}}}}}");. Run tests and capture output to a file. Grep the output for "tddy": to find collected markers. Populate markers (expected) and marker_results (collected vs expected). test_output_file is the path where test output was saved.

**test_command**: Derive from the project (Cargo.toml → cargo test, package.json → npm test, pytest.ini → pytest, etc.).
**prerequisite_actions**: Suggest the cheapest approach. If the test command already compiles/builds (e.g. cargo test compiles first), use "None". Only suggest explicit build steps when needed (e.g. "npm install" before "npm test").
**run_single_or_selected_tests**: How to run a single test or filter by name/pattern (e.g. `cargo test <name>`, `pytest -k <pattern>`, `npm test -- --testNamePattern=<pattern>`)."#
        .to_string()
}

pub fn build_prompt(prd_content: &str, acceptance_tests_content: &str) -> String {
    format!(
        "Create skeleton code and failing lower-level tests based on this PRD and acceptance tests:\n\n## PRD\n\n{}\n\n## Acceptance Tests\n\n{}",
        prd_content, acceptance_tests_content
    )
}

/// Build the follow-up prompt when the user has answered clarification questions.
pub fn build_followup_prompt(
    prd_content: &str,
    acceptance_tests_content: &str,
    answers: &str,
) -> String {
    format!(
        r#"Here are the user's answers to your questions:

{answers}

Now create skeleton code and failing tests based on this PRD and acceptance tests:

## PRD

{prd}

## Acceptance Tests

{at}"#,
        answers = answers.trim(),
        prd = prd_content,
        at = acceptance_tests_content
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_includes_red_goal_instructions() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("red") || prompt.contains("skeleton"),
            "system prompt must instruct Claude for red goal"
        );
    }
}
