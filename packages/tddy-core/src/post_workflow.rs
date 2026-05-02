//! Post-workflow elicitation: GitHub PR and optional session worktree removal before
//! [`WorkflowComplete`](crate::presenter::WorkflowEvent::WorkflowComplete).
//!
//! Pure policy helpers shared by presenters, persistence merge, and future orchestration
//! (`workflow_runner`). No I/O — callers persist via `changeset.yaml` / [`Context`].

use log::{debug, trace};

fn github_pr_phase_is_terminal_for_reprompt(phase: &str) -> bool {
    matches!(phase, "published" | "failed" | "declined" | "skipped_no_pr")
}

/// Ordered elicitation step ids: GitHub PR first, then conditional session worktree removal.
pub fn post_workflow_elicitation_step_order() -> Vec<&'static str> {
    trace!(
        target: "tddy_core::post_workflow",
        "post_workflow_elicitation_step_order"
    );
    vec!["github_pr", "session_worktree_removal"]
}

/// Whether the session worktree removal prompt may be shown (only after a successful PR publish
/// path when the user opted into opening a PR).
pub fn should_prompt_session_worktree_removal(
    user_consented_to_pr: bool,
    pr_status_phase: &str,
) -> bool {
    debug!(
        target: "tddy_core::post_workflow",
        "should_prompt_session_worktree_removal: consented_to_pr={user_consented_to_pr} phase={pr_status_phase:?}"
    );
    user_consented_to_pr && pr_status_phase == "published"
}

/// After resume, whether the GitHub PR elicitation should run again (`false` when stored PR state
/// is terminal).
pub fn should_reprompt_github_pr_on_resume(persisted_pr_phase: Option<&str>) -> bool {
    match persisted_pr_phase {
        None => {
            trace!(
                target: "tddy_core::post_workflow",
                "should_reprompt_github_pr_on_resume: no persisted phase — allow prompt"
            );
            true
        }
        Some(phase) if github_pr_phase_is_terminal_for_reprompt(phase) => {
            trace!(
                target: "tddy_core::post_workflow",
                "should_reprompt_github_pr_on_resume: terminal phase {:?} — skip",
                phase
            );
            false
        }
        Some(phase) => {
            debug!(
                target: "tddy_core::post_workflow",
                "should_reprompt_github_pr_on_resume: non-terminal phase {:?} — allow resume/reprompt handling",
                phase
            );
            true
        }
    }
}

/// Operator-visible line for PR automation status (plain CLI, structured logs, presenter payloads).
///
/// Stable English phrases preferred; keep free of stray newlines so TUI overlays stay safe where
/// this string is multiplexed alongside ratatui (callers avoid `println!` in TUI mode per project rules).
pub fn post_workflow_pr_status_display_line(
    phase: &str,
    url: Option<&str>,
    error: Option<&str>,
) -> Option<String> {
    debug!(
        target: "tddy_core::post_workflow",
        "post_workflow_pr_status_display_line: phase={phase:?}"
    );

    match phase.trim() {
        "pushing_branch" => Some(
            "GitHub PR: pushing session branch to remote before opening pull request.".to_string(),
        ),
        "published" => {
            let url = url.map(str::trim).filter(|u| !u.is_empty());
            Some(match url {
                Some(u) => format!("GitHub PR opened: {u}"),
                None => "GitHub PR: pull request published successfully.".to_string(),
            })
        }
        "failed" => {
            let detail = error
                .map(str::trim)
                .filter(|e| !e.is_empty())
                .unwrap_or("unknown error");
            Some(format!(
                "GitHub PR automation failed ({detail}). Inspect auth, scopes, and remote state."
            ))
        }
        other => Some(format!(
            "GitHub PR status: phase={other} — automation in progress."
        )),
    }
}
