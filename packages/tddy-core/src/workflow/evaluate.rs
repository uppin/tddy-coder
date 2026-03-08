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
7. ALWAYS end your response with a structured-response block — REQUIRED.

**CRITICAL**: The content between <structured-response> and </structured-response> MUST be exactly one valid JSON object. Do NOT output a number, array, or numbered list items inside the block. The parser expects a single JSON object starting with {"goal":"evaluate-changes",...}.

Read the JSON Schema file at `schemas/evaluate.schema.json` in the working directory for the exact output format specification. Your final output MUST include this exact block (replace placeholders with actual values).
For build_results status use: "pass", "fail", or "not_run" (when build could not be executed).

<structured-response content-type="application-json" schema="schemas/evaluate.schema.json">
{"goal":"evaluate-changes","summary":"<human-readable summary>","risk_level":"<low|medium|high|critical>","build_results":[{"package":"<name>","status":"<pass|fail|not_run>","notes":null}],"issues":[{"severity":"<info|warning|error>","category":"<code_quality|test_infrastructure|...>","file":"<path>","line":<number>,"description":"<text>","suggestion":"<optional>"}],"changeset_sync":{"status":"<synced|not_found|...>","items_updated":0,"items_added":0},"files_analyzed":[{"file":"<path>","lines_changed":<number>,"changeset_item":null}],"test_impact":{"tests_affected":0,"new_tests_needed":0},"changed_files":[{"path":"<path>","change_type":"<modified|added|removed>","lines_added":0,"lines_removed":0}],"affected_tests":[{"path":"<path>","status":"<created|updated|removed|skipped>","description":"<text>"}],"validity_assessment":"<detailed assessment of whether the change is valid for the intended use-case>"}
</structured-response>"#
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
    eprintln!(
        r#"{{"tddy":{{"marker_id":"M008b","scope":"workflow::evaluate::build_prompt","data":{{}}}}}}"#
    );
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
    fn system_prompt_references_schema_and_includes_schema_attribute() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("schemas/evaluate.schema.json"),
            "system prompt must reference evaluate schema file"
        );
        assert!(
            prompt.contains("schema=\"schemas/evaluate.schema.json\""),
            "system prompt example must include schema= attribute"
        );
    }
}
