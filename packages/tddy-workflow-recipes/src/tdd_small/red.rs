//! Merged red phase (acceptance-style work + failing tests) for `tdd-small`.
//!
//! Prompts are recipe-owned and must not track [`crate::tdd::red::system_prompt`] verbatim.

/// System prompt for the merged `red` step on the `tdd-small` recipe.
pub fn merged_red_system_prompt() -> String {
    log::debug!("merged_red_system_prompt: building tdd-small merged red system prompt");
    r#"You are a **tdd-small merged red** assistant: one backend turn that both captures acceptance-style intent from the PRD and produces skeleton code with failing tests.

This is **not** the classic full-`tdd` red phase prompt — the `tdd-small` recipe merges acceptance exploration and red work to reduce runner hand-offs.

You MUST:
1. Derive or refine acceptance criteria from the PRD (and `acceptance-tests.md` when present) so the feature scope is explicit.
2. Plan implementation structure (traits, structs, modules) aligned with those criteria.
3. Add skeleton production code that compiles (`todo!()`, `unimplemented!()`, or equivalent).
4. Add failing lower-level tests that exercise the new code paths.
5. Run the project's test command and confirm the new tests fail as expected.
6. Submit with `tddy-tools submit --goal red --data '<your JSON output>'` (same schema as classic red: see `tddy-tools get-schema red`).

If you need clarification, use `tddy-tools ask` with structured questions.

**Logging markers**: At production skeleton entry points, emit a single-line JSON marker with a `"tddy"` key so runs can be grepped; never place such markers in test-only files."#
        .to_string()
}

/// User-facing prompt for merged red (PRD + optional acceptance tests body).
pub fn build_merged_red_prompt(prd_content: &str, acceptance_tests_content: &str) -> String {
    format!(
        "tdd-small merged red: use the PRD and acceptance material below. If acceptance tests are empty, derive criteria from the PRD before writing skeletons and failing tests.\n\n## PRD\n\n{}\n\n## Acceptance tests (optional)\n\n{}",
        prd_content, acceptance_tests_content
    )
}

/// Follow-up merged red prompt after `tddy-tools ask` answers.
pub fn build_merged_red_followup_prompt(
    prd_content: &str,
    acceptance_tests_content: &str,
    answers: &str,
) -> String {
    format!(
        r#"The user answered your clarification questions:

{answers}

Continue the **tdd-small merged red** step: update acceptance understanding if needed, then skeletons and failing tests.

## PRD

{prd}

## Acceptance tests (optional)

{at}"#,
        answers = answers.trim(),
        prd = prd_content,
        at = acceptance_tests_content
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_red_system_prompt_identifies_recipe() {
        let p = merged_red_system_prompt();
        assert!(
            p.contains("tdd-small merged red"),
            "merged red must identify tdd-small"
        );
    }
}
