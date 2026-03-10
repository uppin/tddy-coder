//! Evaluate-changes goal prompt and system prompt construction.

/// Return the system prompt for the evaluate-changes goal.
pub fn system_prompt() -> String {
    r#"You are a code review assistant. Analyze the current git changes in the working directory for risks, code quality issues, changed files, affected tests, and overall validity.

Do NOT use ExitPlanMode or EnterPlanMode. If you cannot run the build (e.g. cargo check) due to permission restrictions, use status: "not_run" in build_results and proceed with read-only analysis.

You MUST:
1. Inspect the git diff (e.g. git diff, git diff --staged) to see what changed
2. Run the build (e.g. cargo build, cargo check) to verify compilation
3. Assess risk level: low, medium, high, or critical
4. List all changed files with change_type (modified/added/removed) and line counts
5. List all affected tests (created, updated, removed, skipped)
6. Provide a validity assessment: does the change address the intended use-case?
7. When done, submit your output by calling:
  tddy-tools submit --schema schemas/evaluate.schema.json --data '<your JSON output>'

If you need to ask the user clarification questions, call:
  tddy-tools ask --data '{"questions":[{"header":"...","question":"...","options":[...],"multiSelect":false}]}'
The call will block until the user answers. The response contains the user's answers.

Read the JSON Schema file at `schemas/evaluate.schema.json` in the working directory for the exact output format. The JSON must be a single object starting with {"goal":"evaluate-changes",...} — no number, array, or numbered list items.
For build_results status use: "pass", "fail", or "not_run" (when build could not be executed)."#
    .to_string()
}

/// Build the user-facing prompt for evaluate-changes.
///
/// - `prd_content`: optional PRD text from plan_dir
/// - `changeset_content`: optional changeset YAML text from plan_dir
///
/// When neither is provided the prompt asks the agent to analyze git diff standalone.
/// When plan context is provided it is embedded for changeset-sync analysis.
pub fn build_prompt(prd_content: Option<&str>, changeset_content: Option<&str>) -> String {
    match (prd_content, changeset_content) {
        (Some(prd), Some(changeset)) => format!(
            r#"Analyze the current git changes for risks, changed files, affected tests, and validity. Use the following plan context for changeset-sync analysis:

## PRD

{prd}

## Changeset

{changeset}

Inspect the git diff, run the build, list all changed files and affected tests, and produce an evaluation report with risk level, issues, and validity assessment."#,
            prd = prd,
            changeset = changeset
        ),
        (Some(prd), None) => format!(
            r#"Analyze the current git changes for risks, changed files, affected tests, and validity. Use the following PRD for context:

## PRD

{prd}

Inspect the git diff, run the build, list all changed files and affected tests, and produce an evaluation report with risk level, issues, and validity assessment."#,
            prd = prd
        ),
        (None, Some(changeset)) => format!(
            r#"Analyze the current git changes for risks, changed files, affected tests, and validity. Use the following changeset for context:

## Changeset

{changeset}

Inspect the git diff, run the build, list all changed files and affected tests, and produce an evaluation report with risk level, issues, and validity assessment."#,
            changeset = changeset
        ),
        (None, None) => "Analyze the current git changes in this directory for risks and code quality. Inspect the git diff, run the build (e.g. cargo build or cargo check), list all changed files and affected tests, and produce an evaluation report with risk level, issues, changed files, affected tests, and validity assessment.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_references_schema_and_includes_tddy_tools_submit() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("schemas/evaluate.schema.json"),
            "system prompt must reference evaluate schema file"
        );
        assert!(
            prompt.contains("tddy-tools submit") && prompt.contains("schemas/evaluate.schema.json"),
            "system prompt must instruct agent to use tddy-tools submit with schema"
        );
    }
}
