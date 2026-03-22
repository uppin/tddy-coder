//! Refactor goal prompt and system prompt construction.

/// Return the system prompt for the refactor goal.
///
/// Instructs the agent to execute refactoring tasks from refactoring-plan.md
/// in priority order, running tests after each change.
pub fn system_prompt() -> String {
    r#"You are a refactoring assistant. Execute the refactoring tasks from refactoring-plan.md.

For each task:
1. Read the relevant code
2. Apply the refactoring
3. Run tests to ensure no regressions (`cargo test` or equivalent)
4. Move to the next task

You MUST:
1. Execute tasks in priority order (critical → high → medium → low)
2. Run tests after each change
3. Stop if tests fail and report which task caused the failure
4. When done, submit your output by calling:
  tddy-tools submit --goal refactor --data '<your JSON output>'

If you need to ask the user clarification questions, call:
  tddy-tools ask --data '{"questions":[{"header":"...","question":"...","options":[...],"multiSelect":false}]}'
The call will block until the user answers. The response contains the user's answers.

Run `tddy-tools get-schema refactor` to see the expected output format. The JSON must be a single object starting with {"goal":"refactor",...}."#
    .to_string()
}

/// Build the user-facing prompt for the refactor goal.
///
/// - `refactoring_plan_content`: content of refactoring-plan.md from session_dir
///
/// The prompt instructs the agent to execute all tasks in priority order.
pub fn build_prompt(refactoring_plan_content: &str) -> String {
    format!(
        r#"Execute the refactoring tasks from the following plan. Work through each task in priority order, running tests after each change.

## Refactoring Plan

{refactoring_plan}

Execute all tasks. Report summary, number of tasks completed, and whether tests pass."#,
        refactoring_plan = refactoring_plan_content
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_references_schema() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("tddy-tools get-schema refactor"),
            "system prompt must reference get-schema for refactor"
        );
        assert!(
            prompt.contains("tddy-tools submit") && prompt.contains("--goal refactor"),
            "system prompt must instruct agent to use tddy-tools submit --goal refactor"
        );
    }

    #[test]
    fn build_prompt_includes_plan_content() {
        let prompt = build_prompt("## Tasks\n- Rename method");
        assert!(
            prompt.contains("Rename method"),
            "prompt must include plan content"
        );
    }
}
