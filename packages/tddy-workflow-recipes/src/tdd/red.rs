//! Red goal prompt and system prompt construction.

pub fn system_prompt() -> String {
    r#"You are a TDD Red phase assistant. Read the PRD and acceptance tests, then plan the implementation and create skeleton code with failing tests.

You MUST:
1. Plan the implementation structure (new traits, structs, methods, modules) based on the PRD and acceptance tests
2. Create skeleton code that compiles: new interfaces/structs/methods with unimplemented bodies (use todo!(), unimplemented!(), or default returns as appropriate for the language)
3. Write failing lower-level tests (unit/integration) that test the planned code paths at a granular level
4. Run the project's test command (e.g. cargo test for Rust) to verify all new tests fail
5. Remove or adjust any tests that pass - passing tests do not verify new behavior
6. When done, submit your output by calling:
  tddy-tools submit --goal red --data '<your JSON output>'

If you need to ask the user clarification questions, call:
  tddy-tools ask --data '{"questions":[{"header":"...","question":"...","options":[...],"multiSelect":false}]}'
The call will block until the user answers. The response contains the user's answers.

Run `tddy-tools get-schema red` to see the expected output format. The JSON must be a single object starting with {"goal":"red",...} — no number, array, or numbered list items.

The summary must describe what skeletons and tests were created and confirm all tests are failing. The tests array lists each failing test. The skeletons array lists each skeleton (trait, struct, method, function, or module) added.

**Logging markers**: At each skeleton entry point, add an eprintln! (or equivalent) that outputs JSON with a unique "tddy" key, e.g. eprintln!("{{\"tddy\":{{\"marker_id\":\"M001\",\"scope\":\"module::fn\",\"data\":{{}}}}}}");. Run tests and capture output to a file. Grep the output for "tddy": to find collected markers. Populate markers (expected) and marker_results (collected vs expected). test_output_file is the path where test output was saved.

**test_command**: Derive from the project (Cargo.toml → cargo test, package.json → npm test, pytest.ini → pytest, etc.).
**prerequisite_actions**: Suggest the cheapest approach. If the test command already compiles/builds (e.g. cargo test compiles first), use "None". Only suggest explicit build steps when needed (e.g. "npm install" before "npm test").
**run_single_or_selected_tests**: How to run a single test or filter by name/pattern (e.g. `cargo test <name>`, `pytest -k <pattern>`, `npm test -- --testNamePattern=<pattern>`).

**Production-only logging markers**: Logging markers MUST NOT appear in test code (unit tests, integration tests under `tests/`, `#[cfg(test)]` modules, or language-specific test file conventions). Place markers only at entry points in **production** source or skeleton code that new tests exercise; never in test-only files.

**Marker lifecycle (cleanup)**: JSON `tddy` / `marker_id` emissions are for Red verification only. They MUST NOT remain in merge-ready code. The **refactor** goal removes them from production sources (delete the `eprintln!`/`println!`/equivalent lines and any code used solely to emit markers), unless `refactoring-plan.md` already tracks that work — then complete it there."#
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

Now create skeleton code and failing tests based on this PRD and acceptance tests.

**Production-only logging markers**: Logging markers MUST NOT appear in test code. Place markers only at **production** skeleton entry points.

**Marker lifecycle**: Remove JSON `tddy` / `marker_id` emissions in the **refactor** goal (see system prompt).

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

    #[test]
    fn system_prompt_references_schema_and_includes_tddy_tools_submit() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("tddy-tools get-schema red"),
            "system prompt must reference get-schema for red"
        );
        assert!(
            prompt.contains("tddy-tools submit") && prompt.contains("--goal red"),
            "system prompt must instruct agent to use tddy-tools submit --goal red"
        );
    }

    /// Red system prompt must forbid placing logging markers in test code (acceptance / PRD).
    #[test]
    fn system_prompt_forbids_markers_in_test_code() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("MUST NOT")
                && prompt.contains("test code")
                && prompt.contains("production"),
            "system prompt must forbid markers in test code and tie markers to production skeleton entry points"
        );
    }

    #[test]
    fn system_prompt_requires_refactor_cleanup_for_red_markers() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("Marker lifecycle")
                && prompt.contains("refactor")
                && prompt.contains("marker_id"),
            "system prompt must tie Red JSON marker removal to the refactor goal"
        );
    }
}
