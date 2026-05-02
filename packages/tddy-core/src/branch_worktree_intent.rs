//! Branch vs worktree intent: persisted under `changeset.yaml` → `workflow` (PRD).

use log::{debug, info};

use crate::changeset::{BranchWorktreeIntent, Changeset, ChangesetWorkflow};
use crate::workflow::context::Context;

/// Resolved naming/checkout plan from persisted workflow + changeset fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeBranchPlan {
    pub checkout_branch: String,
    pub worktree_directory_basename: String,
    /// Integration or chain-PR remote-tracking ref to create a new branch from (`new_branch_from_base` only).
    pub integration_start_ref: Option<String>,
}

/// Validates required workflow fields when `branch_worktree_intent` is set.
pub fn validate_workflow_branch_intent(cs: &Changeset) -> Result<(), String> {
    let Some(ref wf) = cs.workflow else {
        return Ok(());
    };
    let Some(intent) = wf.branch_worktree_intent else {
        return Ok(());
    };
    match intent {
        BranchWorktreeIntent::NewBranchFromBase => {
            if wf
                .new_branch_name
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(
                    "workflow.new_branch_name is required when branch_worktree_intent is new_branch_from_base"
                        .to_string(),
                );
            }
            Ok(())
        }
        BranchWorktreeIntent::WorkOnSelectedBranch => {
            if wf
                .selected_branch_to_work_on
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(
                    "workflow.selected_branch_to_work_on is required when branch_worktree_intent is work_on_selected_branch"
                        .to_string(),
                );
            }
            Ok(())
        }
    }
}

/// Resolve branch name, worktree folder basename, and start ref from changeset + workflow intent.
pub fn resolve_branch_and_worktree_plan(cs: &Changeset) -> Result<WorktreeBranchPlan, String> {
    let wf = cs
        .workflow
        .as_ref()
        .ok_or("changeset has no workflow block")?;
    let intent = wf
        .branch_worktree_intent
        .ok_or("workflow.branch_worktree_intent not set")?;

    let worktree_directory_basename = cs
        .worktree_directory_basename()
        .ok_or("no worktree_suggestion or name for worktree directory")?;

    match intent {
        BranchWorktreeIntent::NewBranchFromBase => {
            let checkout_branch = wf
                .new_branch_name
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or("workflow.new_branch_name is required for new_branch_from_base")?;
            let integration_start_ref = wf
                .selected_integration_base_ref
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or(
                    "workflow.selected_integration_base_ref is required for new_branch_from_base",
                )?;
            info!(
                target: "tddy_core::branch_worktree_intent",
                "resolve_branch_and_worktree_plan: new_branch_from_base checkout_branch={} start={}",
                checkout_branch,
                integration_start_ref
            );
            Ok(WorktreeBranchPlan {
                checkout_branch,
                worktree_directory_basename,
                integration_start_ref: Some(integration_start_ref),
            })
        }
        BranchWorktreeIntent::WorkOnSelectedBranch => {
            let checkout_branch = wf
                .selected_branch_to_work_on
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or(
                    "workflow.selected_branch_to_work_on is required for work_on_selected_branch",
                )?;
            info!(
                target: "tddy_core::branch_worktree_intent",
                "resolve_branch_and_worktree_plan: work_on_selected_branch checkout_branch={}",
                checkout_branch
            );
            Ok(WorktreeBranchPlan {
                checkout_branch,
                worktree_directory_basename,
                integration_start_ref: None,
            })
        }
    }
}

/// Merge persisted workflow intent keys into session [`Context`] for hooks and resume routing.
pub fn merge_branch_worktree_intent_into_context(wf: &ChangesetWorkflow, ctx: &Context) {
    if let Some(intent) = wf.branch_worktree_intent {
        let s = intent.as_str().to_string();
        ctx.set_sync("branch_worktree_intent", s.clone());
        debug!(
            target: "tddy_core::branch_worktree_intent",
            "merge_branch_worktree_intent_into_context: branch_worktree_intent={}",
            s
        );
    }
    if let Some(ref r) = wf.selected_integration_base_ref {
        ctx.set_sync("selected_integration_base_ref", r.clone());
        debug!(
            target: "tddy_core::branch_worktree_intent",
            "merge_branch_worktree_intent_into_context: selected_integration_base_ref len={}",
            r.len()
        );
    }
    if let Some(ref n) = wf.new_branch_name {
        ctx.set_sync("new_branch_name", n.clone());
    }
    if let Some(ref b) = wf.selected_branch_to_work_on {
        ctx.set_sync("selected_branch_to_work_on", b.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changeset::{BranchWorktreeIntent, Changeset, ChangesetWorkflow};

    fn changeset_with_new_branch_intent() -> Changeset {
        Changeset {
            name: Some("N".into()),
            branch_suggestion: Some("feature/ignored".into()),
            worktree_suggestion: Some("wt".into()),
            workflow: Some(ChangesetWorkflow {
                run_optional_step_x: Some(false),
                demo_options: vec![],
                tool_schema_id: Some("urn:tddy:tool/changeset-workflow".into()),
                branch_worktree_intent: Some(BranchWorktreeIntent::NewBranchFromBase),
                selected_integration_base_ref: Some("origin/main".into()),
                new_branch_name: Some("feature/custom-from-intent".into()),
                selected_branch_to_work_on: None,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn green_resolves_new_branch_from_base_to_named_branch() {
        let plan = resolve_branch_and_worktree_plan(&changeset_with_new_branch_intent())
            .expect("GREEN: intent resolves to a worktree branch plan");
        assert_eq!(plan.checkout_branch, "feature/custom-from-intent");
    }
}
