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
4. ALWAYS end your response with a structured-response block — REQUIRED.

**CRITICAL**: The content between <structured-response> and </structured-response> MUST be exactly one valid JSON object starting with {"goal":"refactor",...}.

Read the JSON Schema file at `schemas/refactor.schema.json` in the working directory for the exact output format specification.

<structured-response content-type="application-json" schema="schemas/refactor.schema.json">
{"goal":"refactor","summary":"<human-readable summary of refactoring results>","tasks_completed":<number>,"tests_passing":<true|false>}
</structured-response>"#
    .to_string()
}

/// Build the user-facing prompt for the refactor goal.
///
/// - `refactoring_plan_content`: content of refactoring-plan.md from plan_dir
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
            prompt.contains("schemas/refactor.schema.json"),
            "system prompt must reference refactor schema file"
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
