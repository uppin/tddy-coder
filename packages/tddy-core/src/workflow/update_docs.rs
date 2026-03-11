//! Update-docs goal prompt and system prompt construction.
//!
//! Instructs the agent to read planning artifacts and update the target repo's
//! documentation per repo guidelines.

/// Return the system prompt for the update-docs goal.
///
/// Instructs the agent to read PRD, changeset, progress, and other artifacts,
/// then update feature docs, dev docs, changelogs per repo guidelines.
pub fn system_prompt() -> String {
    r#"You are a documentation update assistant. Read the planning artifacts from the plan directory and update the target repo's documentation.

**Sources** (read from plan directory):
- PRD.md — Product requirements (what was built)
- progress.md — Implementation status
- changeset.yaml — Workflow state, sessions
- acceptance-tests.md — Test definitions
- evaluation-report.md — Change analysis
- refactoring-plan.md — What was refactored

**Targets** (update in the project root, per repo guidelines):
- Feature docs: docs/ft/{product-area}/
- Dev docs: packages/*/docs/, packages/*/README.md
- Changelog: packages/*/docs/changesets.md, docs/ft/*/changelog.md

**Process**:
1. Read all available artifacts
2. Discover the repo's documentation structure
3. Extract final state (State B) — no delta language ("changed", "updated", "now")
4. Apply content transformations to target docs
5. Update changelog/changesets history
6. When done, submit your output by calling: tddy-tools submit --schema schemas/update-docs.schema.json --data '<your JSON output>'

Read the JSON Schema file at `schemas/update-docs.schema.json` in the working directory for the exact output format. The JSON must include: goal, summary, docs_updated.

**CRITICAL**: You MUST call tddy-tools submit with your complete output. Do NOT embed structured output in text. The submit call delivers the output to the workflow — if you do not call it, the workflow fails."#
        .to_string()
}

/// Build the user-facing prompt for the update-docs goal.
///
/// - `artifacts_summary`: Summary of available artifacts (paths and brief descriptions)
///
/// The prompt instructs the agent to update documentation per repo guidelines.
pub fn build_prompt(artifacts_summary: &str) -> String {
    format!(
        r#"Update the repository documentation based on the planning artifacts.

## Available Artifacts

{artifacts}

The plan directory contains the artifacts above. The working directory is the project root. Discover the repo's documentation structure (docs/ft/, packages/*/docs/, etc.) and update it to reflect the final state of the implementation. Use State B language — no "changed", "updated", or "now" phrasing. Add changelog entries as appropriate.

Report summary and number of docs updated."#,
        artifacts = artifacts_summary
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_references_schema() {
        let prompt = system_prompt();
        assert!(
            prompt.contains("schemas/update-docs.schema.json"),
            "system prompt must reference update-docs schema file"
        );
    }

    #[test]
    fn build_prompt_includes_artifacts() {
        let prompt = build_prompt("PRD.md: Product requirements");
        assert!(
            prompt.contains("PRD.md"),
            "prompt must include artifact content"
        );
    }
}
