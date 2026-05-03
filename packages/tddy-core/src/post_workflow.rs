//! Post-workflow elicitation: GitHub PR and optional session worktree removal before
//! [`WorkflowComplete`](crate::presenter::WorkflowEvent::WorkflowComplete).
//!
//! Pure policy helpers shared by presenters, persistence merge, and future orchestration
//! (`workflow_runner`). No I/O — callers persist via `changeset.yaml` / [`Context`].

use log::{debug, trace};

use crate::backend::{ClarificationQuestion, QuestionOption};
use crate::changeset::ChangesetWorkflow;

fn github_pr_phase_is_terminal_for_reprompt(phase: &str) -> bool {
    matches!(phase, "published" | "failed" | "declined" | "skipped_no_pr")
}

/// Label for the “open PR” answer on the post-workflow GitHub PR prompt (must match option label).
pub const GITHUB_PR_OPERATOR_LABEL_YES: &str = "Yes, open a GitHub pull request";
/// Label for skipping PR automation at post-workflow elicitation.
pub const GITHUB_PR_OPERATOR_LABEL_NO: &str = "No, skip opening a PR";

/// Label: remove the session git worktree after a published PR.
pub const SESSION_WORKTREE_LABEL_YES: &str = "Yes, remove session worktree";
/// Label: keep the worktree on disk.
pub const SESSION_WORKTREE_LABEL_NO: &str = "No, keep worktree";

/// Single-select question: operator consent before GitHub PR automation (runs from [`crate::presenter::workflow_runner`]).
#[must_use]
pub fn github_pr_operator_question() -> ClarificationQuestion {
    ClarificationQuestion {
        header: "Post-workflow".to_string(),
        question: "Open a GitHub pull request for this session branch? (Requires GitHub auth / MCP when automation runs.)".to_string(),
        options: vec![
            QuestionOption {
                label: GITHUB_PR_OPERATOR_LABEL_YES.to_string(),
                description: "Proceed toward PR automation as configured in your environment.".to_string(),
            },
            QuestionOption {
                label: GITHUB_PR_OPERATOR_LABEL_NO.to_string(),
                description: "Skip PR for this session (recorded in changeset).".to_string(),
            },
        ],
        multi_select: false,
        allow_other: false,
    }
}

/// Single-select: optional session worktree removal after PR **`published`** (policy in [`should_prompt_session_worktree_removal`]).
#[must_use]
pub fn session_worktree_removal_question() -> ClarificationQuestion {
    ClarificationQuestion {
        header: "Post-workflow".to_string(),
        question: "Remove this session's git worktree from disk?".to_string(),
        options: vec![
            QuestionOption {
                label: SESSION_WORKTREE_LABEL_YES.to_string(),
                description:
                    "Deletes only the worktree path for this session (not the branch on remote)."
                        .to_string(),
            },
            QuestionOption {
                label: SESSION_WORKTREE_LABEL_NO.to_string(),
                description: "Keep the worktree directory.".to_string(),
            },
        ],
        multi_select: false,
        allow_other: false,
    }
}

/// Whether the workflow runner should block on the GitHub PR operator prompt before [`WorkflowComplete`](crate::presenter::WorkflowEvent::WorkflowComplete).
#[must_use]
pub fn post_workflow_github_pr_operator_elicitation_pending(
    wf: Option<&ChangesetWorkflow>,
) -> bool {
    let Some(wf) = wf else {
        return false;
    };
    if wf.post_workflow_open_github_pr != Some(true) {
        return false;
    }
    match &wf.github_pr_status {
        None => true,
        Some(st) => {
            let p = st.phase.as_str();
            if p.is_empty() {
                return true;
            }
            if matches!(p, "in_progress" | "pushing_branch") {
                return false;
            }
            should_reprompt_github_pr_on_resume(Some(p))
        }
    }
}

/// Whether to show the session worktree removal prompt (recipe flag + PR phase + not yet answered).
#[must_use]
pub fn post_workflow_session_worktree_elicitation_pending(
    wf: Option<&ChangesetWorkflow>,
    user_consented_to_github_pr: bool,
) -> bool {
    let Some(wf) = wf else {
        return false;
    };
    if wf.post_workflow_remove_session_worktree != Some(true) {
        return false;
    }
    if wf.operator_remove_session_worktree.is_some() {
        return false;
    }
    let phase = wf
        .github_pr_status
        .as_ref()
        .map(|s| s.phase.as_str())
        .unwrap_or("");
    should_prompt_session_worktree_removal(user_consented_to_github_pr, phase)
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

#[cfg(test)]
mod post_workflow_elicitation_tests {
    use super::*;
    use crate::changeset::{ChangesetWorkflow, GithubPrStatus};

    #[test]
    fn github_pr_operator_pending_when_open_pr_true_and_no_status() {
        let wf = ChangesetWorkflow {
            post_workflow_open_github_pr: Some(true),
            ..Default::default()
        };
        assert!(post_workflow_github_pr_operator_elicitation_pending(Some(
            &wf
        )));
    }

    #[test]
    fn github_pr_operator_not_pending_when_phase_in_progress() {
        let wf = ChangesetWorkflow {
            post_workflow_open_github_pr: Some(true),
            github_pr_status: Some(GithubPrStatus {
                phase: "in_progress".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!post_workflow_github_pr_operator_elicitation_pending(Some(
            &wf
        )));
    }

    #[test]
    fn session_worktree_pending_only_when_published_and_flag_true() {
        let wf = ChangesetWorkflow {
            post_workflow_remove_session_worktree: Some(true),
            github_pr_status: Some(GithubPrStatus {
                phase: "published".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(post_workflow_session_worktree_elicitation_pending(
            Some(&wf),
            true
        ));
        assert!(!post_workflow_session_worktree_elicitation_pending(
            Some(&wf),
            false
        ));
    }

    #[test]
    fn session_worktree_not_pending_after_operator_answer() {
        let wf = ChangesetWorkflow {
            post_workflow_remove_session_worktree: Some(true),
            github_pr_status: Some(GithubPrStatus {
                phase: "published".into(),
                ..Default::default()
            }),
            operator_remove_session_worktree: Some(false),
            ..Default::default()
        };
        assert!(!post_workflow_session_worktree_elicitation_pending(
            Some(&wf),
            true
        ));
    }
}
