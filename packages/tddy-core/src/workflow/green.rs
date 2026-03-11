//! Green goal prompt and system prompt construction.

/// Build the green goal system prompt.
///
/// When `run_demo` is true and demo-plan.md exists, the agent runs the demo by executing
/// a pre-made shell script (e.g. demo.sh) in the plan directory using tools. The script
/// must launch the app in its own terminal window. See AGENTS.md.
pub fn system_prompt(run_demo: bool) -> String {
    let demo_instruction = if run_demo {
        "**demo_results** (required when demo-plan.md exists): Run the demo by executing the pre-made shell script in the plan directory (e.g. ./demo.sh or bash demo.sh). The script must launch the app in its own terminal window. Use tools (Bash) to run it. Report summary and steps_completed in demo_results."
    } else {
        "**demo_results**: The user chose to skip the demo. Do NOT run demo steps. Omit demo_results from your output."
    };
    format!(
        r#"You are a TDD Green phase assistant. Read the progress.md file to understand which tests fail and which skeletons need implementation. Implement production-grade code to make all failing tests pass.

You MUST:
1. Read progress.md for the list of failing tests and skeleton implementations
2. Implement production-grade code (not stubs or workarounds) to make each failing test pass
3. Add detailed logging (log::debug!, log::info!) to reveal flows and system state during development — these will be cleaned in later phases
4. After implementing, run the project's test command (e.g. cargo test) to verify all tests pass
5. Run acceptance tests to verify end-to-end behavior
6. When done, submit your output by calling:
  tddy-tools submit --schema schemas/green.schema.json --data '<your JSON output>'

If you need to ask the user clarification questions, call:
  tddy-tools ask --data '{{"questions":[{{"header":"...","question":"...","options":[...],"multiSelect":false}}]}}'
The call will block until the user answers. The response contains the user's answers.

Read the JSON Schema file at `schemas/green.schema.json` in the working directory for the exact output format. The JSON must be a single object starting with {{"goal":"green",...}} — no number, array, or numbered list items.

The summary must describe what was implemented and confirm test results. The tests array lists each test with status "passing" or "failing"; include "reason" for failing tests. The implementations array lists each implemented item (struct, method, etc.).

{}"#,
        demo_instruction
    )
}

pub fn build_prompt(
    progress_content: &str,
    prd_content: Option<&str>,
    acceptance_tests_content: Option<&str>,
) -> String {
    let mut out = String::from("Implement production code to make all failing tests pass. Use progress.md as the primary guide:\n\n## Progress\n\n");
    out.push_str(progress_content);
    if let Some(prd) = prd_content {
        out.push_str("\n\n## PRD (context)\n\n");
        out.push_str(prd);
    }
    if let Some(at) = acceptance_tests_content {
        out.push_str("\n\n## Acceptance Tests (context)\n\n");
        out.push_str(at);
    }
    out
}

/// Build the follow-up prompt when the user has answered clarification questions.
pub fn build_followup_prompt(
    progress_content: &str,
    answers: &str,
    prd_content: Option<&str>,
    acceptance_tests_content: Option<&str>,
) -> String {
    let mut out = format!(
        r#"Here are the user's answers to your questions:

{}

Now implement production code based on progress.md:

## Progress

{}"#,
        answers.trim(),
        progress_content
    );
    if let Some(prd) = prd_content {
        out.push_str("\n\n## PRD (context)\n\n");
        out.push_str(prd);
    }
    if let Some(at) = acceptance_tests_content {
        out.push_str("\n\n## Acceptance Tests (context)\n\n");
        out.push_str(at);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_includes_green_goal_instructions() {
        let prompt = system_prompt(true);
        assert!(
            prompt.contains("green") || prompt.contains("Implement"),
            "system prompt must instruct Claude for green goal"
        );
    }

    #[test]
    fn system_prompt_references_schema_and_includes_tddy_tools_submit() {
        let prompt = system_prompt(true);
        assert!(
            prompt.contains("schemas/green.schema.json"),
            "system prompt must reference green schema file"
        );
        assert!(
            prompt.contains("tddy-tools submit") && prompt.contains("schemas/green.schema.json"),
            "system prompt must instruct agent to use tddy-tools submit with schema"
        );
    }
}
