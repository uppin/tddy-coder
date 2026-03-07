//! Planning step prompt and system prompt construction.

pub fn system_prompt() -> String {
    r#"You are a technical planning assistant. Analyze the feature description and produce:

1. A PRD (Product Requirements Document) in markdown format
2. A TODO list in markdown format with implementation milestones

If you need clarification before creating the PRD, use the AskUserQuestion tool.

Otherwise, you MUST include a structured-response block with your output. Use this exact format:

<structured-response content-type="application-json">
{"goal": "plan", "prd": "<PRD markdown content>", "todo": "<TODO markdown content>"}
</structured-response>

The prd and todo values must be JSON strings (escape quotes and newlines as needed). The PRD should include: Summary, Background, Requirements, Acceptance Criteria. The TODO should list discrete implementation tasks in dependency order using - [ ] for pending and [x] for completed.

You may include explanatory text before or after the structured-response block, but the block itself is required for parsing."#
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
