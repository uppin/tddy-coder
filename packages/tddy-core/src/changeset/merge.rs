//! What a stored changeset means for the run about to happen.
//!
//! Merging persisted workflow state into a session [`Context`], and resolving the goal, model
//! and agent a continuation should use.

use super::io::read_changeset;
use super::model::{
    Changeset, ChangesetWorkflow, ClarificationQa, ClarificationQuestionForQa, QuestionOptionForQa,
    SessionEntry, StateTransition,
};
use crate::error::WorkflowError;
use crate::workflow::context::Context;
use crate::workflow::ids::{GoalId, WorkflowState};
use crate::workflow::recipe::WorkflowRecipe;
use log::{debug, info};
use std::collections::BTreeMap;
use std::path::Path;
use tddy_workflow::questions::ClarificationQuestion;

/// Merges persisted workflow/demo fields from `changeset.yaml` into session [`Context`]
/// (`run_optional_step_x`, demo options) so graph predicates match stored intent after interview/plan.
///
/// Legacy changesets without a `workflow` block leave context unchanged for these keys; the graph
/// then uses the same defaults as an empty context (e.g. post-green routing per existing graph rules).
pub fn merge_persisted_workflow_into_context(
    session_dir: &Path,
    context: &Context,
) -> Result<(), WorkflowError> {
    info!(
        target: "tddy_core::changeset",
        "merge_persisted_workflow_into_context: session_dir={}",
        session_dir.display()
    );
    let cs = read_changeset(session_dir)?;
    let Some(ref wf) = cs.workflow else {
        debug!(
            target: "tddy_core::changeset",
            "merge_persisted_workflow_into_context: no workflow block in changeset — skipping"
        );
        return Ok(());
    };
    if let Some(b) = wf.run_optional_step_x {
        debug!(
            target: "tddy_core::changeset",
            "merge_persisted_workflow_into_context: set run_optional_step_x={}",
            b
        );
        context.set_sync("run_optional_step_x", b);
    }
    if !wf.demo_options.is_empty() {
        info!(
            target: "tddy_core::changeset",
            "merge_persisted_workflow_into_context: set demo_options count={}",
            wf.demo_options.len()
        );
        context.set_sync("demo_options", wf.demo_options.clone());
    }
    if let Some(ref id) = wf.tool_schema_id {
        debug!(
            target: "tddy_core::changeset",
            "merge_persisted_workflow_into_context: tool_schema_id present (len={})",
            id.len()
        );
    }
    crate::branch_worktree_intent::merge_branch_worktree_intent_into_context(wf, context);
    merge_post_workflow_into_context(wf, context)?;
    Ok(())
}

/// Copies post-workflow PR / worktree elicitation fields from persisted [`ChangesetWorkflow`] into
/// [`Context`] so presenters, CLI, and graph predicates see the same durable state as `changeset.yaml`.
fn merge_post_workflow_into_context(
    wf: &ChangesetWorkflow,
    context: &Context,
) -> Result<(), WorkflowError> {
    if let Some(flag) = wf.post_workflow_open_github_pr {
        debug!(
            target: "tddy_core::changeset",
            "merge_post_workflow_into_context: post_workflow_open_github_pr={flag}"
        );
        context.set_sync("post_workflow_open_github_pr", flag);
    }
    if let Some(flag) = wf.post_workflow_remove_session_worktree {
        debug!(
            target: "tddy_core::changeset",
            "merge_post_workflow_into_context: post_workflow_remove_session_worktree={flag}"
        );
        context.set_sync("post_workflow_remove_session_worktree", flag);
    }
    if let Some(flag) = wf.operator_remove_session_worktree {
        debug!(
            target: "tddy_core::changeset",
            "merge_post_workflow_into_context: operator_remove_session_worktree={flag}"
        );
        context.set_sync("operator_remove_session_worktree", flag);
    }
    let Some(status) = &wf.github_pr_status else {
        return Ok(());
    };
    let v = serde_json::to_value(status).map_err(|e| {
        WorkflowError::ChangesetInvalid(format!(
            "workflow.github_pr_status is not serializable for Context merge: {e}"
        ))
    })?;
    info!(
        target: "tddy_core::changeset",
        "merge_post_workflow_into_context: github_pr_status phase={}",
        status.phase
    );
    context.set_sync("github_pr_status", v);
    Ok(())
}

/// Which workflow goal to run when continuing from an on-disk session (CLI resume, presenter).
///
/// For a normal persisted state, this is [`WorkflowRecipe::next_goal_for_state`]. When the session
/// should be treated as **failed resume** ([`super::model::ChangesetState::current`] is `Failed`, or the last
/// history entry is `Failed`), `next_goal_for_state(Failed)` is `None`; in that case we walk
/// [`super::model::ChangesetState::history`] from newest to oldest, skipping `Failed`, and use the first
/// transition whose [`WorkflowRecipe::next_goal_for_state`] is `Some` and **not** equal to
/// [`WorkflowRecipe::start_goal`].
///
/// Skipping transitions per [`WorkflowRecipe::skip_failed_resume_transition`] avoids a common bad
/// ordering: a full workflow restart writes `Planning` immediately before `Failed`, while an earlier
/// transition (e.g. `GreenImplementing`) still reflects the real phase to retry. If every candidate
/// is skipped or `None`, falls back to [`WorkflowRecipe::start_goal`], with a TDD-specific fallback
/// to **`plan`** when only trailing `Planning` → `plan` entries were skipped (failed during plan).
///
/// When the **last** history entry is `Failed`, the same walk is used even if [`super::model::ChangesetState::current`]
/// was left stale (e.g. still `Planning` after a manual `changeset.yaml` edit that fixed `history`
/// but not `current`). Otherwise `next_goal_for_state(current)` would incorrectly return `plan`.
pub fn start_goal_for_session_continue(recipe: &dyn WorkflowRecipe, cs: &Changeset) -> GoalId {
    let start = recipe.start_goal();
    let history_ends_in_failed = cs
        .state
        .history
        .last()
        .is_some_and(|t| t.state.as_str() == "Failed");
    let use_failed_resume_walk = cs.state.current.as_str() == "Failed" || history_ends_in_failed;
    if !use_failed_resume_walk {
        return recipe
            .next_goal_for_state_with_changeset(&cs.state.current, cs)
            .unwrap_or_else(|| start.clone());
    }
    let mut tdd_skipped_trailing_planning = false;
    for transition in cs.state.history.iter().rev() {
        if transition.state.as_str() == "Failed" {
            continue;
        }
        match recipe.next_goal_for_state_with_changeset(&transition.state, cs) {
            None => continue,
            Some(g) if recipe.skip_failed_resume_transition(&transition.state, &g) => {
                if recipe.name() == "tdd"
                    && transition.state.as_str() == "Planning"
                    && g.as_str() == "plan"
                {
                    tdd_skipped_trailing_planning = true;
                }
                continue;
            }
            Some(g) => return g,
        }
    }
    if recipe.name() == "tdd" && tdd_skipped_trailing_planning {
        return GoalId::new("plan");
    }
    start
}

/// Resolve model for a goal: CLI override > changeset `models` > optional recipe defaults.
pub fn resolve_model(
    changeset: Option<&Changeset>,
    goal: &str,
    cli_model: Option<&str>,
    recipe_defaults: Option<&BTreeMap<String, String>>,
) -> Option<String> {
    if let Some(m) = cli_model {
        return Some(m.to_string());
    }
    if let Some(c) = changeset {
        if let Some(m) = c.models.get(goal) {
            return Some(m.clone());
        }
    }
    recipe_defaults.and_then(|d| d.get(goal).cloned())
}

/// Get session ID for a tag (e.g. "plan" or "impl").
pub fn get_session_for_tag(changeset: &Changeset, tag: &str) -> Option<String> {
    changeset
        .sessions
        .iter()
        .rfind(|s| s.tag == tag)
        .map(|s| s.id.clone())
}

/// Backend `agent` from the latest session with `tag == preferred_tag`, else the last session entry.
pub fn resolve_agent_from_changeset(changeset: &Changeset, preferred_tag: &str) -> Option<String> {
    changeset
        .sessions
        .iter()
        .rfind(|s| s.tag == preferred_tag)
        .map(|s| s.agent.clone())
        .or_else(|| changeset.sessions.last().map(|s| s.agent.clone()))
}

/// Update workflow state only (no new session).
pub fn update_state(changeset: &mut Changeset, new_state: WorkflowState) {
    let now = chrono::Utc::now().to_rfc3339();
    changeset.state.history.push(StateTransition {
        state: new_state.clone(),
        at: now.clone(),
    });
    changeset.state.current = new_state;
    changeset.state.updated_at = now;
}

/// Build clarification_qa from backend questions and newline-separated answers.
pub fn clarification_qa_from_backend(
    questions: Vec<ClarificationQuestion>,
    answers: &str,
) -> Vec<ClarificationQa> {
    let answer_lines: Vec<String> = answers.split('\n').map(|s| s.trim().to_string()).collect();
    questions
        .into_iter()
        .enumerate()
        .map(|(i, q)| {
            let answer = answer_lines.get(i).cloned().unwrap_or_default();
            ClarificationQa {
                question: ClarificationQuestionForQa {
                    header: q.header,
                    question: q.question,
                    options: q
                        .options
                        .into_iter()
                        .map(|o| QuestionOptionForQa {
                            label: o.label,
                            description: o.description,
                        })
                        .collect(),
                    multi_select: q.multi_select,
                },
                answer,
            }
        })
        .collect()
}

/// Append a session and update state.
pub fn append_session_and_update_state(
    changeset: &mut Changeset,
    session_id: String,
    tag: &str,
    new_state: WorkflowState,
    agent: &str,
    system_prompt_file: Option<String>,
) {
    let now = chrono::Utc::now().to_rfc3339();
    changeset.sessions.push(SessionEntry {
        id: session_id.clone(),
        agent: agent.to_string(),
        tag: tag.to_string(),
        created_at: now.clone(),
        system_prompt_file,
    });
    changeset.state.session_id = Some(session_id);
    changeset.state.history.push(StateTransition {
        state: new_state.clone(),
        at: now.clone(),
    });
    changeset.state.current = new_state;
    changeset.state.updated_at = now;
}

#[cfg(test)]
mod resolve_agent_tests {
    use super::*;

    #[test]
    fn resolve_agent_prefers_plan_tag_session() {
        let mut cs = Changeset::default();
        append_session_and_update_state(
            &mut cs,
            "a".into(),
            "plan",
            WorkflowState::new("Planned"),
            "cursor",
            None,
        );
        append_session_and_update_state(
            &mut cs,
            "b".into(),
            "acceptance-tests",
            WorkflowState::new("AcceptanceTestsReady"),
            "claude",
            None,
        );
        assert_eq!(
            resolve_agent_from_changeset(&cs, "plan").as_deref(),
            Some("cursor")
        );
    }

    #[test]
    fn resolve_agent_falls_back_to_last_session() {
        let mut cs = Changeset::default();
        append_session_and_update_state(
            &mut cs,
            "x".into(),
            "acceptance-tests",
            WorkflowState::new("AcceptanceTestsReady"),
            "stub",
            None,
        );
        assert_eq!(
            resolve_agent_from_changeset(&cs, "plan").as_deref(),
            Some("stub")
        );
    }
}
