//! Planning step prompt and system prompt construction.

pub fn system_prompt() -> String {
    r#"You are a technical planning assistant. Analyze the feature description and produce:

1. A PRD (Product Requirements Document) in markdown format
2. A TODO list in markdown format with implementation milestones

If you need clarification before creating the PRD, either use the AskUserQuestion tool OR output a structured block with your questions:

<clarification-questions content-type="application-json">
{"questions":[{"header":"<section>","question":"<text>","options":[{"label":"<choice>","description":"<desc>"}],"multiSelect":false}]}
</clarification-questions>

Otherwise, you MUST include a structured-response block with your output. Read the JSON Schema file at `schemas/plan.schema.json` in the working directory for the exact output format specification. Use this exact format:

<structured-response content-type="application-json" schema="schemas/plan.schema.json">
{"goal": "plan", "name": "<human-readable changeset name>", "prd": "<PRD markdown content>", "todo": "<TODO markdown content>", "discovery": {"toolchain": {"<tool>": "<version>"}, "scripts": {"<name>": "<command>"}, "doc_locations": ["<path>"], "relevant_code": [{"path": "<path>", "reason": "<why>"}], "test_infrastructure": {"runner": "<cmd>", "conventions": "<pattern>"}}, "demo_plan": {"demo_type": "cli|api|ui", "setup_instructions": "<text>", "steps": [{"description": "<text>", "command_or_action": "<cmd>", "expected_result": "<text>"}], "verification": "<text>"}}
</structured-response>

The prd and todo values must be JSON strings (escape quotes and newlines as needed). The PRD should include: Summary, Background, Requirements, Acceptance Criteria, and a Testing Plan section. The Testing Plan must contain: (1) test level determination (E2E/Integration/Unit) with rationale, (2) a list of acceptance tests with descriptive names, (3) target test file paths (existing or new), (4) strong assertions for each test. The TODO should list discrete implementation tasks in dependency order using - [ ] for pending and [x] for completed.

**name** (optional): A short, human-readable name for the changeset (e.g. "Auth Feature", "Stable session dir"). This appears in changeset.yaml and helps identify the session; the session directory itself is a UUID managed by the system.

**branch_suggestion** (optional): When running in daemon mode, suggest a git branch name for the feature (e.g. "feature/auth", "feat/stable-session-dir"). Used for worktree creation.

**worktree_suggestion** (optional): When running in daemon mode, suggest a worktree directory name (e.g. "feature-auth"). Used for git worktree add.

**discovery** (optional): Inspect the project to populate toolchain (e.g. rust, cargo, node), scripts (test, lint), doc_locations, relevant_code paths, and test_infrastructure. The working directory is the project root; read Cargo.toml, package.json, packages/, etc. directly. The plan schema path is provided in the user prompt.

**demo_plan** (optional): When the feature has a user-facing demo (CLI, API, UI), include demo_type, setup_instructions, steps with description/command_or_action/expected_result, and verification criteria.

**CRITICAL**: Your response MUST contain the <structured-response> block with the actual PRD and TODO content. Do NOT return a summary, meta-commentary, or description of what you created. The parser extracts the block directly — if it is missing, the workflow fails. You may add brief explanatory text before the block, but the block itself is mandatory."#
        .to_string()
}

pub fn build_prompt(feature: &str) -> String {
    format!(
        "Create a PRD and implementation TODO for the following feature:\n\n{}",
        feature
    )
}

/// Build the follow-up prompt when the user has answered clarification questions.
pub fn build_followup_prompt(feature: &str, answers: &str) -> String {
    format!(
        r#"Here are the user's answers to your questions:

{answers}

Now create the PRD and TODO for: {feature}"#,
        answers = answers.trim(),
        feature = feature
    )
}

/// Build the prompt when the user has requested plan refinement (plan approval gate).
pub fn build_refinement_prompt(feature: &str, feedback: &str) -> String {
    format!(
        r#"The user has reviewed the plan and requested refinements:

{feedback}

Please revise the PRD and TODO accordingly for: {feature}"#,
        feedback = feedback.trim(),
        feature = feature
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_includes_testing_plan_requirements() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("Testing Plan") || prompt.to_lowercase().contains("testing plan"),
            "system prompt must require Testing Plan section in PRD"
        );
    }

    #[test]
    fn system_prompt_references_schema_and_includes_schema_attribute() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("schemas/plan.schema.json"),
            "system prompt must reference plan schema file"
        );
        assert!(
            prompt.contains("schema=\"schemas/plan.schema.json\""),
            "system prompt example must include schema= attribute"
        );
    }

    /// System prompt must not mention plan_dir_suggestion and must guide agent to provide
    /// a human-readable changeset `name` instead.
    ///
    /// Fails until `plan_dir_suggestion` is removed from the prompt and `name` guidance is added.
    #[test]
    fn test_planning_prompt_mentions_name_not_plan_dir_suggestion() {
        let prompt = system_prompt();
        assert!(
            !prompt.contains("plan_dir_suggestion"),
            "system prompt must not reference 'plan_dir_suggestion' after R2 removal; \
             the field should be removed from the prompt template"
        );
        // The prompt must instruct the agent to set the `name` field of the changeset
        assert!(
            prompt.contains("\"name\"") || prompt.contains("`name`") || prompt.contains("name"),
            "system prompt must guide agent to provide a changeset 'name' field"
        );
    }
}
