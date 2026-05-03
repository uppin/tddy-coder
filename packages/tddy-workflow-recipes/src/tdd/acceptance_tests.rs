//! Acceptance tests step prompt and system prompt construction.

pub fn system_prompt() -> String {
    r#"You are an acceptance test creation assistant. Read the Testing Plan section from the PRD and create acceptance tests.

You MUST:
1. Create each acceptance test fully implemented (not stubs) - tests must fail because production code is missing
2. Run the project's test command to verify all new tests fail (Red state). Use the appropriate command for the project: `cargo test` for Rust, `npm test` or `npx jest` for Node/TypeScript, `pytest` for Python, etc.
3. Delete or adjust any tests that pass - passing tests do not verify new behavior
4. Do NOT ask for permission to write files - you have write access. Create the test files directly.
5. When done, submit your output by calling:
  tddy-tools submit --goal acceptance-tests --data-stdin << 'EOF'
<your JSON output>
EOF

Use --data-stdin and a heredoc. Do NOT use --data with inline JSON for large payloads. Do NOT use Write, cat, or python to build the JSON first — put the JSON directly in the heredoc.

If you need to ask the user clarification questions, call:
  tddy-tools ask --data '{"questions":[{"header":"...","question":"...","options":[...],"multiSelect":false}]}'
The call will block until the user answers. The response contains the user's answers.

Run `tddy-tools get-schema acceptance-tests` to see the expected output format. The JSON must be a single object starting with {"goal":"acceptance-tests",...} — no number, array, or numbered list items.

**Session action manifests (required)** — In addition to acceptance test source files, write at least three distinct YAML manifests under `actions/` in the session directory. Each file MUST conform to the session action manifest schema (`version`, `id`, `summary`, `architecture`, `command`, optional `input_schema`, `output_schema`, `result_kind`, `output_path_arg`; unknown keys are rejected). Use `tddy-tools get-schema` against manifest tooling if needed, and keep `tddy-tools get-schema acceptance-tests` for your submit JSON shape.

The three manifests MUST cover these scopes explicitly:
- **single-test** — run one named test (e.g. filtered `cargo test` / `./dev cargo test <name>`).
- **selected acceptance** — run exactly the acceptance tests you authored for this goal (filters matching tests listed in your submit `tests` array).
- **package** or **crate** — run full tests per affected package (Rust: `cargo test -p <crate>` / `./dev cargo test -p …`; Node: workspace filter equivalent).

After creating or editing each manifest, run `tddy-tools list-actions` to verify discovery, then `tddy-tools invoke-action` with minimal safe `--data` (`{}` when no schema fields are required) to test-drive the manifest before submit. Record any failures in prose in your summary.

**CRITICAL**: You MUST call tddy-tools submit with your complete structured output (summary, tests array, test_command, etc.). Do NOT return a summary, meta-commentary, or description of what you created without calling submit. The submit call delivers the output to the workflow — if you do not call it, the workflow fails.

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
            prompt.contains("tddy-tools get-schema acceptance-tests"),
            "system prompt must reference get-schema for acceptance-tests"
        );
        assert!(
            prompt.contains("tddy-tools submit")
                && prompt.contains("--goal acceptance-tests")
                && prompt.contains("--data-stdin"),
            "system prompt must instruct agent to use tddy-tools submit --goal acceptance-tests with --data-stdin (heredoc), like plan"
        );
    }

    #[test]
    fn acceptance_tests_prompt_requires_three_session_actions() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("actions/"),
            "system prompt must require writing manifests under actions/"
        );
        assert!(
            prompt.contains("invoke-action"),
            "system prompt must require tddy-tools invoke-action test-drives"
        );
        assert!(
            prompt.contains("tddy-tools list-actions") || prompt.contains("list-actions"),
            "system prompt must require enumerating manifests (list-actions)"
        );
        assert!(
            prompt.contains("single-test") || prompt.contains("single test"),
            "system prompt must call out single-test scope"
        );
        assert!(
            prompt.contains("selected") && prompt.contains("acceptance"),
            "system prompt must call out selected acceptance tests scope"
        );
        assert!(
            prompt.contains("package") || prompt.contains("crate"),
            "system prompt must call out package- or crate-scoped full test runs"
        );
        assert!(
            prompt.contains("tddy-tools get-schema"),
            "system prompt must reference get-schema for manifest or acceptance-tests JSON contract"
        );
        assert!(
            prompt.contains("tddy-tools submit")
                && prompt.contains("--goal acceptance-tests")
                && prompt.contains("--data-stdin"),
            "regression: submit path for acceptance-tests goal must remain documented"
        );
    }

    /// Parity with `planning::system_prompt` and `update_docs::system_prompt`: agents must not
    /// finish with a prose summary instead of `tddy-tools submit`, or the workflow cannot proceed.
    #[test]
    fn system_prompt_mandates_tddy_tools_submit_for_workflow_delivery() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("if you do not call it, the workflow fails"),
            "acceptance-tests system prompt must state that omitting tddy-tools submit fails the workflow (same contract as plan and update-docs)"
        );
    }
}
