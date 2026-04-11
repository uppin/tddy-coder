//! PRD Testing Plan: merge-pr and tdd-small prompts mention **tddy-tools** GitHub PR tools when authenticated.

use serial_test::serial;
use tddy_workflow_recipes::merge_pr::merge_pr_github_tools_awareness_line;
use tddy_workflow_recipes::{
    merged_red_system_prompt, tdd_small_github_pr_tools_awareness_sentence,
};

#[test]
fn merge_pr_hooks_prompt_mentions_github_pr_tools_when_authenticated() {
    let line = merge_pr_github_tools_awareness_line(true);
    assert!(
        !line.trim().is_empty(),
        "merge-pr hooks must inject non-empty GitHub PR tools awareness when authenticated"
    );
    assert!(
        line.contains("tddy-tools"),
        "awareness must name tddy-tools; got: {line:?}"
    );
    assert!(
        line.to_lowercase().contains("github"),
        "awareness must mention GitHub; got: {line:?}"
    );
    assert!(
        line.contains("pull request") || line.contains("PR"),
        "awareness must reference pull requests; got: {line:?}"
    );
}

#[test]
#[serial]
fn tdd_small_system_prompt_includes_github_pr_tools_awareness() {
    std::env::set_var("GITHUB_TOKEN", "ghp_acceptance_test_not_real");
    struct Clear;
    impl Drop for Clear {
        fn drop(&mut self) {
            std::env::remove_var("GITHUB_TOKEN");
        }
    }
    let _clear = Clear;

    // Build merged prompt first so logging markers for both helpers are exercised before assertions.
    let prompt = merged_red_system_prompt();
    let awareness = tdd_small_github_pr_tools_awareness_sentence();
    assert!(
        !awareness.trim().is_empty(),
        "tdd-small must expose non-empty GitHub PR tools awareness text for authenticated sessions"
    );
    assert!(
        prompt.contains(awareness),
        "merged red system prompt must include the GitHub PR tools awareness sentence"
    );
    assert!(
        prompt.contains("GitHub") && prompt.contains("tddy-tools"),
        "merged red system prompt must mention GitHub and tddy-tools for PR tooling; got len {}",
        prompt.len()
    );
}
