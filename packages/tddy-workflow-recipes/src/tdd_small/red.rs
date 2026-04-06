//! Merged red phase (acceptance-style work + failing tests) for `tdd-small`.
//!
//! Prompts are recipe-owned and must not track [`crate::tdd::red::system_prompt`] verbatim.

use crate::github_rest_common::github_env_token_present;

/// Sentence surfaced to agents when GitHub credentials are available (paired with **tddy-tools** MCP PR tools).
pub const TDD_SMALL_GITHUB_PR_TOOLS_AWARENESS: &str = "With an authenticated GitHub session (**GITHUB_TOKEN** or **GH_TOKEN**), use **tddy-tools** MCP GitHub PR tools to create or update pull requests instead of ad-hoc shell scripts.";

/// Single sentence used when appending GitHub PR tooling guidance (same condition as merge-pr hooks: token set).
#[must_use]
pub fn tdd_small_github_pr_tools_awareness_sentence() -> &'static str {
    log::debug!(
        "tdd_small_github_pr_tools_awareness_sentence: returning static awareness sentence"
    );
    TDD_SMALL_GITHUB_PR_TOOLS_AWARENESS
}

/// System prompt for the merged `red` step on the `tdd-small` recipe.
pub fn merged_red_system_prompt() -> String {
    log::debug!("merged_red_system_prompt: building tdd-small merged red system prompt");
    let mut s = String::from(
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

**Logging markers**: At production skeleton entry points, emit a single-line JSON marker with a `"tddy"` key so runs can be grepped; never place such markers in test-only files."#,
    );

    if github_env_token_present() {
        let awareness = tdd_small_github_pr_tools_awareness_sentence();
        s.push_str("\n\n## GitHub PR tools\n\n");
        s.push_str(awareness);
        s.push('\n');
        log::info!(
            "merged_red_system_prompt: appended GitHub PR awareness (len={})",
            awareness.len()
        );
    } else {
        log::debug!(
            "merged_red_system_prompt: no GITHUB_TOKEN/GH_TOKEN — omitting GitHub PR tools section"
        );
    }
    s
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
    use serial_test::serial;

    #[test]
    fn merged_red_system_prompt_identifies_recipe() {
        let p = merged_red_system_prompt();
        assert!(
            p.contains("tdd-small merged red"),
            "merged red must identify tdd-small"
        );
    }

    #[test]
    #[serial]
    fn merged_red_omits_github_section_without_token() {
        struct Restore(Option<String>, Option<String>);
        impl Drop for Restore {
            fn drop(&mut self) {
                match &self.0 {
                    Some(v) => std::env::set_var("GITHUB_TOKEN", v),
                    None => std::env::remove_var("GITHUB_TOKEN"),
                }
                match &self.1 {
                    Some(v) => std::env::set_var("GH_TOKEN", v),
                    None => std::env::remove_var("GH_TOKEN"),
                }
            }
        }
        let _r = Restore(
            std::env::var("GITHUB_TOKEN").ok(),
            std::env::var("GH_TOKEN").ok(),
        );
        std::env::remove_var("GITHUB_TOKEN");
        std::env::remove_var("GH_TOKEN");
        let p = merged_red_system_prompt();
        assert!(
            !p.contains("## GitHub PR tools"),
            "without token, merged red must not claim GitHub PR tools section; got: {p}"
        );
    }

    #[test]
    #[serial]
    fn merged_red_includes_github_section_with_token() {
        struct Restore(Option<String>, Option<String>);
        impl Drop for Restore {
            fn drop(&mut self) {
                match &self.0 {
                    Some(v) => std::env::set_var("GITHUB_TOKEN", v),
                    None => std::env::remove_var("GITHUB_TOKEN"),
                }
                match &self.1 {
                    Some(v) => std::env::set_var("GH_TOKEN", v),
                    None => std::env::remove_var("GH_TOKEN"),
                }
            }
        }
        let _r = Restore(
            std::env::var("GITHUB_TOKEN").ok(),
            std::env::var("GH_TOKEN").ok(),
        );
        std::env::set_var("GITHUB_TOKEN", "ghp_test_not_real");
        let p = merged_red_system_prompt();
        assert!(
            p.contains("## GitHub PR tools") && p.contains(TDD_SMALL_GITHUB_PR_TOOLS_AWARENESS),
            "with token, merged red must include awareness; got len {}",
            p.len()
        );
    }
}
