//! Which goal a run continuing from an on-disk session starts at.
//!
//! It reads the stored [`Changeset`] but answers through the [`WorkflowRecipe`], so it sits with the
//! engine that runs recipes rather than in the changeset model beneath it.

use crate::changeset::Changeset;
use crate::workflow::ids::GoalId;
use crate::workflow::recipe::WorkflowRecipe;

/// Which workflow goal to run when continuing from an on-disk session (CLI resume, presenter).
///
/// For a normal persisted state, this is [`WorkflowRecipe::next_goal_for_state`]. When the session
/// should be treated as **failed resume** ([`crate::changeset::ChangesetState::current`] is `Failed`, or the last
/// history entry is `Failed`), `next_goal_for_state(Failed)` is `None`; in that case we walk
/// [`crate::changeset::ChangesetState::history`] from newest to oldest, skipping `Failed`, and use the first
/// transition whose [`WorkflowRecipe::next_goal_for_state`] is `Some` and **not** equal to
/// [`WorkflowRecipe::start_goal`].
///
/// Skipping transitions per [`WorkflowRecipe::skip_failed_resume_transition`] avoids a common bad
/// ordering: a full workflow restart writes `Planning` immediately before `Failed`, while an earlier
/// transition (e.g. `GreenImplementing`) still reflects the real phase to retry. If every candidate
/// is skipped or `None`, falls back to [`WorkflowRecipe::start_goal`], with a TDD-specific fallback
/// to **`plan`** when only trailing `Planning` → `plan` entries were skipped (failed during plan).
///
/// When the **last** history entry is `Failed`, the same walk is used even if [`crate::changeset::ChangesetState::current`]
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
