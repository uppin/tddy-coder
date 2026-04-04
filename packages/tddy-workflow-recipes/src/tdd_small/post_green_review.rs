//! Single post-green step for `tdd-small`: merged code review concerns from evaluate + validate.

/// System prompt for `post-green-review` (one invoke, one structured submit).
pub fn system_prompt() -> String {
    log::debug!("post_green_review::system_prompt: building merged evaluate+validate prompt");
    r#"You are the **tdd-small post-green review** assistant. After green, perform a single consolidated review that covers both:
- **Evaluate-style concerns**: risk, summary, and whether the changes align with the PRD (validity).
- **Validate-style concerns**: whether test, production-readiness, and clean-code reports were written as appropriate.

When finished, submit exactly once using:
  tddy-tools submit --goal post-green-review --data '<JSON>'

Use `tddy-tools get-schema post-green-review` for the JSON shape. Required fields include goal, summary, risk_level, validity_assessment, and the three `*_written` booleans for reports.

Do not run separate evaluate and validate phases; this one step replaces both for the tdd-small recipe."#
        .to_string()
}

/// Build the user prompt from PRD (optional) and raw changeset text (optional).
pub fn build_prompt(prd: Option<&str>, changeset_yaml: Option<&str>) -> String {
    let mut s = String::from("Perform the merged post-green review.\n\n");
    if let Some(p) = prd {
        s.push_str("## PRD context\n\n");
        s.push_str(p);
        s.push_str("\n\n");
    }
    if let Some(c) = changeset_yaml {
        s.push_str("## changeset.yaml (snapshot)\n\n");
        s.push_str(c);
        s.push('\n');
    }
    s
}
