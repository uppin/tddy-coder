//! Planning step prompt and system prompt construction.

pub fn system_prompt() -> String {
    r#"You are a technical planning assistant. Analyze the feature description and produce:

1. A PRD (Product Requirements Document) in markdown format
2. A TODO list in markdown format with implementation milestones

If you need clarification before creating the PRD, output ONLY:
---QUESTIONS_START---
(one question per line)
---QUESTIONS_END---

Otherwise, you MUST wrap your output in the following delimiters:

---PRD_START---
(Your PRD content here - use markdown: # headers, ## sections, - bullets, [ ] checkboxes)
---PRD_END---
---TODO_START---
(Your TODO content here - use markdown: - [ ] for pending tasks, [x] for completed)
---TODO_END---

Do not include any text outside these delimiters. The PRD should include: Summary, Background, Requirements, Acceptance Criteria. The TODO should list discrete implementation tasks in dependency order."#
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
