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
{"goal": "plan", "prd": "<PRD markdown content>", "todo": "<TODO markdown content>", "discovery": {"toolchain": {"<tool>": "<version>"}, "scripts": {"<name>": "<command>"}, "doc_locations": ["<path>"], "plan_dir_suggestion": "<path>", "relevant_code": [{"path": "<path>", "reason": "<why>"}], "test_infrastructure": {"runner": "<cmd>", "conventions": "<pattern>"}}, "demo_plan": {"demo_type": "cli|api|ui", "setup_instructions": "<text>", "steps": [{"description": "<text>", "command_or_action": "<cmd>", "expected_result": "<text>"}], "verification": "<text>"}}
</structured-response>

The prd and todo values must be JSON strings (escape quotes and newlines as needed). The PRD should include: Summary, Background, Requirements, Acceptance Criteria, and a Testing Plan section. The Testing Plan must contain: (1) test level determination (E2E/Integration/Unit) with rationale, (2) a list of acceptance tests with descriptive names, (3) target test file paths (existing or new), (4) strong assertions for each test. The TODO should list discrete implementation tasks in dependency order using - [ ] for pending and [x] for completed.

**discovery** (optional): Inspect the project to populate toolchain (e.g. rust, cargo, node), scripts (test, lint), doc_locations, plan_dir_suggestion (where to create plan dir, e.g. docs/dev/1-WIP/), relevant_code paths, and test_infrastructure. The working directory is the plan output directory; for project files (Cargo.toml, package.json, etc.), use the parent directory (e.g. Read ../Cargo.toml). **IMPORTANT**: `plan_dir_suggestion` is now acted upon — the workflow will move the plan directory from its staging location to `git_root/plan_dir_suggestion/` after you respond. Inspect the repo for existing plan directories (e.g. docs/, plans/, .dev/), CLAUDE.md conventions, or documentation patterns to decide the best location. Only set `plan_dir_suggestion` to a relative path (no `..`, no leading `/`); leave it absent to keep the default output location.

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
}
