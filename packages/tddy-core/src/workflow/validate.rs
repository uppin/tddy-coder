//! Validate-changes goal prompt and system prompt construction.

/// Return the system prompt for the validate-changes goal.
pub fn system_prompt() -> String {
    r#"You are a code review assistant. Analyze the current git changes in the working directory for risks, code quality issues, and test impact.

Do NOT use ExitPlanMode or EnterPlanMode. If you cannot run the build (e.g. cargo check) due to permission restrictions, use status: "not_run" in build_results and proceed with read-only analysis.

You MUST:
1. Inspect the git diff (e.g. git diff, git diff --staged) to see what changed
2. Run the build (e.g. cargo build, cargo check, npm run build) to verify compilation
3. Assess risk level: low, medium, high, or critical
4. List any issues found (severity, category, file, line, description, suggestion)
5. ALWAYS end your response with a structured-response block — REQUIRED.

**CRITICAL**: The content between <structured-response> and </structured-response> MUST be exactly one valid JSON object. Do NOT output a number, array, or numbered list items inside the block. The parser expects a single JSON object starting with {"goal":"validate-changes",...}.

Read the JSON Schema file at `schemas/validate.schema.json` in the working directory for the exact output format specification. Your final output MUST include this exact block (replace placeholders with actual values).
For build_results status use: "pass", "fail", or "not_run" (when build could not be executed).

<structured-response content-type="application-json" schema="schemas/validate.schema.json">
{"goal":"validate-changes","summary":"<human-readable summary>","risk_level":"<low|medium|high|critical>","build_results":[{"package":"<name>","status":"<pass|fail|not_run>","notes":null}],"issues":[{"severity":"<info|warning|error>","category":"<code_quality|test_infrastructure|...>","file":"<path>","line":<number>,"description":"<text>","suggestion":"<optional>"}],"changeset_sync":{"status":"<synced|not_found|...>","items_updated":0,"items_added":0},"files_analyzed":[{"file":"<path>","lines_changed":<number>,"changeset_item":null}],"test_impact":{"tests_affected":0,"new_tests_needed":0}}
</structured-response>"#
        .to_string()
}

/// Build the user-facing prompt for validate-changes.
///
/// - `prd_content`: optional PRD text from plan_dir
/// - `changeset_content`: optional changeset YAML text from plan_dir
///
/// When neither is provided the prompt asks the agent to analyze git diff standalone.
/// When plan context is provided it is embedded for changeset-sync analysis.
pub fn build_prompt(prd_content: Option<&str>, changeset_content: Option<&str>) -> String {
    match (prd_content, changeset_content) {
        (Some(prd), Some(changeset)) => format!(
            r#"Analyze the current git changes for risks and code quality. Use the following plan context for changeset-sync analysis:

## PRD

{prd}

## Changeset

{changeset}

Inspect the git diff, run the build, and produce a validation report with risk level, issues, and build results."#,
            prd = prd,
            changeset = changeset
        ),
        (Some(prd), None) => format!(
            r#"Analyze the current git changes for risks and code quality. Use the following PRD for context:

## PRD

{prd}

Inspect the git diff, run the build, and produce a validation report with risk level, issues, and build results."#,
            prd = prd
        ),
        (None, Some(changeset)) => format!(
            r#"Analyze the current git changes for risks and code quality. Use the following changeset for context:

## Changeset

{changeset}

Inspect the git diff, run the build, and produce a validation report with risk level, issues, and build results."#,
            changeset = changeset
        ),
        (None, None) => "Analyze the current git changes in this directory for risks and code quality. Inspect the git diff, run the build (e.g. cargo build or cargo check), and produce a validation report with risk level, issues, and build results.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_references_schema_and_includes_schema_attribute() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("schemas/validate.schema.json"),
            "system prompt must reference validate schema file"
        );
        assert!(
            prompt.contains("schema=\"schemas/validate.schema.json\""),
            "system prompt example must include schema= attribute"
        );
    }
}
