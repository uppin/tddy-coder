//! Planning step prompt and system prompt construction.

pub fn system_prompt() -> String {
    r#"You are a technical planning assistant. Analyze the feature description and produce a single PRD (Product Requirements Document) in markdown format.

If you need clarification before creating the PRD, call:
  tddy-tools ask --data '{"questions":[{"header":"<section>","question":"<text>","options":[{"label":"<choice>","description":"<desc>"}],"multiSelect":false}]}'
The call will block until the user answers. The response contains the user's answers.

When done, submit your output by calling:
  tddy-tools submit --schema schemas/plan.schema.json --data '<your JSON output>'

Read the JSON Schema file at `schemas/plan.schema.json` in the working directory for the exact output format. The JSON must include: goal, prd, and optionally name, discovery, demo_plan.

**PRD structure** — The prd value is a single JSON string (escape quotes and newlines as needed). The PRD must include these sections in order:

1. **Summary** — Brief overview of the feature
2. **Background** — Context and motivation
3. **Requirements** — Functional and non-functional requirements
4. **Acceptance Criteria** — Conditions that must be met
5. **Testing Plan** — (1) test level determination (E2E/Integration/Unit) with rationale, (2) acceptance tests with descriptive names, (3) target test file paths, (4) strong assertions for each test
6. **TODO** — Work needed to fulfill the product requirement (implementation tasks for the feature). Do NOT list workflow steps (e.g. red phase, green phase, run tests). Use - [ ] for pending and [x] for completed. List discrete tasks in dependency order.

The TODO section is part of the PRD body, not a separate field. Example:

## TODO

- [ ] Create auth module
- [ ] Implement login endpoint
- [ ] Implement logout endpoint

**name** (optional): A short, human-readable name for the changeset (e.g. "Auth Feature", "Stable session dir"). This appears in changeset.yaml and helps identify the session; the session directory itself is a UUID managed by the system.

**branch_suggestion** (optional): When running in daemon mode, suggest a git branch name for the feature (e.g. "feature/auth", "feat/stable-session-dir"). Used for worktree creation.

**worktree_suggestion** (optional): When running in daemon mode, suggest a worktree directory name (e.g. "feature-auth"). Used for git worktree add.

**discovery** (optional): Inspect the project to populate toolchain (e.g. rust, cargo, node), scripts (test, lint), doc_locations, relevant_code paths, and test_infrastructure. The working directory is the project root; read Cargo.toml, package.json, packages/, etc. directly. The plan schema path is provided in the user prompt.

**demo_plan** (optional): When the feature has a user-facing demo (CLI, API, UI), include demo_type, setup_instructions, steps with description/command_or_action/expected_result, and verification criteria.

**CRITICAL**: You MUST call tddy-tools submit with your complete PRD content (including the TODO section). Do NOT return a summary, meta-commentary, or description of what you created. The submit call delivers the output to the workflow — if you do not call it, the workflow fails."#
        .to_string()
}

pub fn build_prompt(feature: &str) -> String {
    format!(
        "Create a PRD (including ## TODO section) for the following feature:\n\n{}",
        feature
    )
}

/// Build the follow-up prompt when the user has answered clarification questions.
pub fn build_followup_prompt(feature: &str, answers: &str) -> String {
    format!(
        r#"Here are the user's answers to your questions:

{answers}

Now create the PRD (including ## TODO section) for: {feature}"#,
        answers = answers.trim(),
        feature = feature
    )
}

/// Build the prompt when the user has requested plan refinement (plan approval gate).
pub fn build_refinement_prompt(feature: &str, feedback: &str) -> String {
    format!(
        r#"The user has reviewed the plan and requested refinements:

{feedback}

Please revise the PRD (including ## TODO section) accordingly for: {feature}"#,
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
    fn system_prompt_references_schema_and_includes_tddy_tools_submit() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("schemas/plan.schema.json"),
            "system prompt must reference plan schema file"
        );
        assert!(
            prompt.contains("tddy-tools submit") && prompt.contains("schemas/plan.schema.json"),
            "system prompt must instruct agent to use tddy-tools submit with schema"
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
