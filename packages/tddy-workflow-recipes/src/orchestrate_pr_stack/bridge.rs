//! Bridges plan-pr-stack output into an orchestrate-pr-stack session.
//!
//! After a `plan-pr-stack` session completes and writes `stack-plan.yaml`, the operator
//! creates an `orchestrate-pr-stack` session. On the first `assess` tick the orchestrator
//! calls [`seed_orchestrator_stack_from_plan`] lazily: if `Changeset.stack` is `None` but
//! `stack-plan.yaml` exists in the session dir, this function reads, validates, and persists
//! the plan as `Stack` nodes on the orchestrator changeset.

use std::path::Path;

use tddy_core::WorkflowError;

use crate::plan_pr_stack::StackPlanOutput;

/// Seed the orchestrator's `Changeset.stack` from a completed `StackPlanOutput`.
///
/// Reads the plan (passed directly — callers read `stack-plan.yaml` and parse it), validates
/// it, converts to `StackNode`s, and atomically persists the `Stack` on the orchestrator.
/// No-ops if the plan is empty; returns `Err` on validation failure or write failure.
pub fn seed_orchestrator_stack_from_plan(
    orchestrator_session_dir: &Path,
    plan: &StackPlanOutput,
) -> Result<(), WorkflowError> {
    if plan.prs.is_empty() {
        return Ok(());
    }
    // Validate before touching disk.
    crate::plan_pr_stack::validate_stack_plan(plan).map_err(|e| {
        WorkflowError::ChangesetInvalid(format!("seed_orchestrator_stack_from_plan: {e}"))
    })?;

    let nodes = crate::plan_pr_stack::planned_prs_into_stack_nodes(&plan.prs);
    tddy_core::changeset::update_stack_atomic(orchestrator_session_dir, |stack| {
        if stack.nodes.is_empty() {
            stack.version = plan.version;
            stack.nodes = nodes.clone();
        }
        // Idempotent: if nodes already populated, don't overwrite.
    })
}

/// Execute a single stack-node merge: write journal, call GithubPrApi::merge_pr, mark node merged.
///
/// This is the core logic of `MergeTask::run`, extracted for direct testability without
/// constructing a `Context`.
pub fn execute_stack_merge(
    orchestrator_session_dir: &Path,
    node_id: &str,
    pr_number: u64,
    gh: &dyn crate::orchestrate_pr_stack::github::GithubPrApi,
) -> Result<String, WorkflowError> {
    use crate::orchestrate_pr_stack::transient::{
        write_stack_op_journal, MergePhase, StackOpJournal,
    };
    use tddy_core::changeset::{update_stack_atomic, GithubPrStatus};

    // 1. Write journal with Planned phase so crash recovery can resume.
    let dependents = {
        let cs = tddy_core::changeset::read_changeset(orchestrator_session_dir)?;
        let stack = cs.stack.as_ref().cloned().unwrap_or_default();
        stack
            .nodes
            .iter()
            .filter(|n| n.parents.contains(&node_id.to_string()))
            .map(|n| n.node_id.clone())
            .collect::<Vec<_>>()
    };

    let journal = StackOpJournal {
        op_id: uuid::Uuid::new_v4().to_string(),
        merged_node_id: node_id.to_string(),
        merge_phase: MergePhase::Planned,
        dependents: dependents.clone(),
    };
    write_stack_op_journal(orchestrator_session_dir, &journal)?;

    // 2. Merge the PR.
    let sha = gh.merge_pr(pr_number)?;

    // 3. Advance journal to PrMerged.
    let journal = StackOpJournal {
        merge_phase: MergePhase::PrMerged { sha: sha.clone() },
        ..journal
    };
    write_stack_op_journal(orchestrator_session_dir, &journal)?;

    // 4. Mark node merged in changeset.
    update_stack_atomic(orchestrator_session_dir, |stack| {
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.pr_status = Some(GithubPrStatus {
                phase: "merged".to_string(),
                url: None,
                error: None,
            });
        }
    })?;

    Ok(sha)
}

/// Repoint all dependents of a merged node: rebase+force-push each branch, patch GitHub PR base.
///
/// Advances the journal through `RepointingDependent { idx }` phases, deletes it on completion.
/// On rebase conflict, marks the dependent node `Failed` and stops (does not abort the journal).
pub fn execute_stack_repoint(
    orchestrator_session_dir: &Path,
    repo_root: &Path,
    merged_node_id: &str,
    dependents: &[String],
    default_branch: &str,
    gh: &dyn crate::orchestrate_pr_stack::github::GithubPrApi,
) -> Result<(), WorkflowError> {
    use crate::orchestrate_pr_stack::git_ops::{force_push_with_lease, merge_base, rebase_onto};
    use crate::orchestrate_pr_stack::transient::{
        delete_stack_op_journal, write_stack_op_journal, MergePhase, StackOpJournal,
    };
    use tddy_core::changeset::{read_changeset, update_stack_atomic, GithubPrStatus};

    for (idx, dep_id) in dependents.iter().enumerate() {
        // Advance journal to current repoint step.
        if let Ok(journal_raw) = std::fs::read_to_string(
            orchestrator_session_dir
                .join(".workflow")
                .join("stack-op.json"),
        ) {
            if let Ok(mut j) = serde_json::from_str::<StackOpJournal>(&journal_raw) {
                j.merge_phase = MergePhase::RepointingDependent { idx };
                let _ = write_stack_op_journal(orchestrator_session_dir, &j);
            }
        }

        // Read the dependent's branch from the stack.
        let cs = read_changeset(orchestrator_session_dir)?;
        let stack = cs.stack.as_ref().cloned().unwrap_or_default();
        let Some(dep_node) = stack.nodes.iter().find(|n| &n.node_id == dep_id).cloned() else {
            continue;
        };
        let dep_branch = match dep_node.branch.as_deref() {
            Some(b) => b.to_string(),
            None => {
                log::warn!("execute_stack_repoint: dependent {dep_id} has no branch; skipping");
                continue;
            }
        };

        // Best-effort git rebase: if the branch doesn't exist locally (e.g. remote-only),
        // skip git ops and proceed to PR base update.
        let merged_branch = stack
            .nodes
            .iter()
            .find(|n| n.node_id == merged_node_id)
            .and_then(|n| n.branch.clone())
            .unwrap_or_else(|| format!("feature/{merged_node_id}"));
        let old_base = merge_base(repo_root, &dep_branch, &merged_branch)
            .unwrap_or_else(|_| default_branch.to_string());

        match rebase_onto(repo_root, default_branch, &old_base, &dep_branch) {
            Ok(()) => {
                // Rebase succeeded — force-push the rebased branch.
                let sha_out = std::process::Command::new("git")
                    .current_dir(repo_root)
                    .args(["rev-parse", &dep_branch])
                    .output()
                    .ok();
                let expected_sha = sha_out
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                if let Err(e) = force_push_with_lease(repo_root, &dep_branch, &expected_sha) {
                    log::warn!("execute_stack_repoint: force-push failed for {dep_branch}: {e}");
                }
            }
            Err(e) => {
                let err_msg = e.to_string();
                // If the branch simply doesn't exist locally, skip git rebase (remote-only branch).
                if err_msg.contains("pathspec") || err_msg.contains("did not match") {
                    log::debug!(
                        "execute_stack_repoint: branch {dep_branch} not local; skipping rebase"
                    );
                } else {
                    // Real rebase conflict — mark dependent failed.
                    update_stack_atomic(orchestrator_session_dir, |stack| {
                        if let Some(node) = stack.nodes.iter_mut().find(|n| &n.node_id == dep_id) {
                            node.pr_status = Some(GithubPrStatus {
                                phase: "error".to_string(),
                                url: None,
                                error: Some(err_msg.clone()),
                            });
                        }
                    })?;
                    return Err(WorkflowError::WriteFailed(format!(
                        "execute_stack_repoint: rebase of {dep_branch} onto {default_branch} failed: {err_msg}"
                    )));
                }
            }
        }

        // Patch the GitHub PR base to default_branch.
        // Try live API first; fall back to extracting PR number from stored pr_status.url.
        let pr_number = gh
            .get_open_pr(&dep_branch)
            .ok()
            .flatten()
            .map(|pr| pr.number)
            .or_else(|| pr_number_from_status_url(dep_node.pr_status.as_ref()));
        if let Some(number) = pr_number {
            if let Err(e) = gh.patch_pr_base(number, default_branch) {
                log::warn!("execute_stack_repoint: patch_pr_base({number}) failed: {e}");
            }
        }
    }

    // Done — delete journal.
    let _ = delete_stack_op_journal(orchestrator_session_dir);
    Ok(())
}

/// Extract the PR number from a GitHub PR URL stored in `GithubPrStatus`.
/// Parses `.../pull/{number}` from the URL.
fn pr_number_from_status_url(status: Option<&tddy_core::changeset::GithubPrStatus>) -> Option<u64> {
    let url = status?.url.as_deref()?;
    url.rsplit('/').next()?.parse::<u64>().ok()
}
