//! Demo goal: system prompt and user prompt construction.
//!
//! The demo step must set `system_prompt` on the context so backends (e.g. Cursor) receive the
//! same `tddy-tools submit` contract as other goals. User-facing text comes from `demo-plan.md`.

/// System prompt for the standalone demo goal (`Goal::Demo`).
pub fn system_prompt() -> String {
    r#"You are a demo assistant. Follow demo-plan.md (provided in the user message): run the demo steps, verify the outcome, and summarize what was demonstrated.

Do NOT use ExitPlanMode or EnterPlanMode.

You MUST:
1. Execute the demo as described (e.g. run scripts, CLI, or steps in the plan)
2. Record what ran, demo type if applicable, and verification of success or failure
3. When done, submit your output by calling:
  tddy-tools submit --goal demo --data '<your JSON output>'

If you need to ask the user clarification questions, call:
  tddy-tools ask --data '{"questions":[{"header":"...","question":"...","options":[...],"multiSelect":false}]}'
The call will block until the user answers. The response contains the user's answers.

Run `tddy-tools get-schema demo` to see the expected output format. The JSON must be a single object starting with {"goal":"demo",...} — no number, array, or numbered list items."#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_includes_submit_and_schema_for_demo_goal() {
        let prompt = system_prompt();
        assert!(
            !prompt.is_empty(),
            "demo system prompt must not be empty so agents receive submit instructions"
        );
        assert!(
            prompt.contains("tddy-tools submit") && prompt.contains("--goal demo"),
            "demo system prompt must require tddy-tools submit --goal demo, got length {}",
            prompt.len()
        );
        assert!(
            prompt.contains("tddy-tools get-schema demo") || prompt.contains("get-schema demo"),
            "demo system prompt must reference get-schema for demo goal"
        );
    }
}
