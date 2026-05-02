//! Acceptance tests (PRD Testing Plan): post-workflow GitHub PR + worktree elicitation.
//!
//! Post-workflow ordering, worktree-prompt gating, resume policy, and PR status copy.

use tddy_core::{
    post_workflow_elicitation_step_order, post_workflow_pr_status_display_line,
    should_prompt_session_worktree_removal, should_reprompt_github_pr_on_resume,
};

/// Two prompts in order: GitHub PR intent first; worktree removal only after a successful PR path.
#[test]
fn post_workflow_elicitation_order_pr_then_worktree() {
    assert_eq!(
        post_workflow_elicitation_step_order(),
        vec!["github_pr", "session_worktree_removal"],
        "PRD: PR elicitation precedes conditional worktree removal"
    );
    assert!(
        !should_prompt_session_worktree_removal(false, "skipped"),
        "declining or skipping the PR path must not offer worktree removal"
    );
    assert!(
        !should_prompt_session_worktree_removal(true, "failed"),
        "failed PR publish must not offer worktree removal"
    );
}

#[test]
fn session_worktree_removal_prompt_after_successful_pr_publish_only() {
    assert!(
        should_prompt_session_worktree_removal(true, "published"),
        "after successful PR publish, operator may be asked about worktree removal"
    );
}

/// Terminal persisted PR phases must not re-elicit on resume.
#[test]
fn resume_does_not_reprompt_terminal_pr_state() {
    for phase in ["published", "failed", "declined", "skipped_no_pr"] {
        assert!(
            !should_reprompt_github_pr_on_resume(Some(phase)),
            "terminal phase {phase:?} must not re-prompt github PR elicitation"
        );
    }
    assert!(
        should_reprompt_github_pr_on_resume(None),
        "no persisted phase implies first run or legacy session — may prompt"
    );
    assert!(
        should_reprompt_github_pr_on_resume(Some("in_progress")),
        "non-terminal automation must allow reprompt/resume handling"
    );
}

/// Status transitions must be representable as operator-visible text (CLI / logs / presenter).
#[test]
fn pr_status_visible_in_events_or_cli() {
    let pushing = post_workflow_pr_status_display_line("pushing_branch", None, None)
        .expect("pushing_branch phase must surface a non-empty status line for the operator");
    let low = pushing.to_lowercase();
    assert!(
        low.contains("push") || low.contains("branch"),
        "expected push progress in line: {pushing:?}"
    );

    let published = post_workflow_pr_status_display_line(
        "published",
        Some("https://github.com/example/test/pull/42"),
        None,
    )
    .expect("published phase must surface success line");
    assert!(
        published.contains("42") || published.contains("github.com"),
        "expected URL or id in line: {published:?}"
    );

    let err_line =
        post_workflow_pr_status_display_line("failed", None, Some("insufficient oauth scope"))
            .expect("failed phase must surface actionable error text");
    assert!(
        err_line.to_lowercase().contains("scope") || err_line.to_lowercase().contains("error"),
        "expected failure reason in line: {err_line:?}"
    );
}
