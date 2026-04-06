//! System prompts and operator-facing documentation for the **review** workflow (branch diff → elicitation → `branch-review` submit).

use super::git_context::merge_base_strategy_documentation;

/// Basename for the persisted review artifact under the session directory ([`super::REVIEW_MD_BASENAME`]).
pub const REVIEW_MD_BASENAME: &str = "review.md";

/// System prompt for the **inspect** task (elicitation; no structured `tddy-tools submit` yet).
#[must_use]
pub fn inspect_system_prompt() -> String {
    format!(
        "{}\n\n\
         You are running the **review** workflow — **Inspect** phase (read-only).\n\
         - Use the git context below to understand what changed on this branch.\n\
         - Ask focused clarification questions via normal agent behavior (structured questions when appropriate).\n\
         - Do **not** emit the final `tddy-tools submit` JSON for `branch-review` during this step; \
         that happens in the **branch-review** step after clarification.\n",
        merge_base_strategy_documentation()
    )
}

/// System prompt for the **branch-review** task (structured submit → `review.md`).
#[must_use]
pub fn branch_review_system_prompt() -> String {
    format!(
        "{}\n\n\
         You are running the **review** workflow — **branch-review** phase.\n\
         - Produce a substantive markdown review aligned with the diff scope above.\n\
         - Complete the step with **`tddy-tools submit --goal branch-review`** using JSON that validates \
         against the branch-review schema (`review_body_markdown` must include headings such as \
         `# Branch review` and `## Findings`).\n",
        merge_base_strategy_documentation()
    )
}
