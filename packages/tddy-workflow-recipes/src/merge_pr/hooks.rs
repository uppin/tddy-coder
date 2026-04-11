//! Workflow hooks for **merge-pr** (`analyze` → `sync-main` → `finalize` → `end`).

use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc;

use tddy_core::backend::AgentOutputSink;
use tddy_core::changeset::{read_changeset, BranchWorktreeIntent};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::TaskResult;

use crate::github_rest_common::github_env_token_present;
use crate::review::{
    format_diff_context_for_prompt, merge_base_commit_for_review, resolve_git_repo_root,
};
use crate::tdd::hooks_common;

use super::{TASK_ANALYZE, TASK_FINALIZE, TASK_SYNC_MAIN};

/// Hooks for merge-pr: read-only analysis, then worktree-isolated sync + finalize.
#[derive(Debug)]
pub struct MergePrWorkflowHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl MergePrWorkflowHooks {
    pub fn new(event_tx: Option<mpsc::Sender<WorkflowEvent>>) -> Self {
        Self { event_tx }
    }
}

impl RunnerHooks for MergePrWorkflowHooks {
    fn agent_output_sink(&self) -> Option<AgentOutputSink> {
        self.event_tx.as_ref().map(|tx| {
            let tx = tx.clone();
            AgentOutputSink::new(move |s: &str| {
                let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
            })
        })
    }

    fn before_task(
        &self,
        task_id: &str,
        context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::debug!("[merge-pr hooks] before_task task_id={task_id}");

        if task_id == TASK_SYNC_MAIN {
            if let Some(ref session_dir) = context.get_sync::<PathBuf>("session_dir") {
                hooks_common::ensure_worktree_for_session(
                    session_dir.as_path(),
                    context,
                    self.event_tx.as_ref(),
                    "[merge-pr hooks] sync-main",
                )?;
            }
        }

        let target_branch = resolve_target_branch_from_changeset(context);

        let working = context
            .get_sync::<PathBuf>("worktree_dir")
            .or_else(|| context.get_sync::<PathBuf>("output_dir"))
            .or_else(|| context.get_sync::<PathBuf>("session_dir"));

        let repo_root: Option<PathBuf> = working.as_ref().and_then(|p| resolve_git_repo_root(p));

        let git_block = if let Some(ref repo) = repo_root {
            let base = merge_base_commit_for_review(repo);
            log::debug!(
                "[merge-pr hooks] repo={} merge_base={}",
                repo.display(),
                base
            );
            format_diff_context_for_prompt(repo, &base)
        } else {
            log::info!(
                "[merge-pr hooks] no git repo from worktree_dir/output_dir/session_dir; diff context omitted"
            );
            "_Git repository not found from session/worktree paths; merge-base diff unavailable._\n"
                .to_string()
        };

        if let Some(prompt) = system_prompt_for_task(task_id, &git_block, target_branch.as_deref())
        {
            context.set_sync("system_prompt", prompt);
        }

        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
            let _ = tx.send(WorkflowEvent::StateChange {
                from: String::new(),
                to: task_id.to_string(),
            });
        }

        log::debug!(
            "[merge-pr hooks] before_task complete task_id={} repo_resolved={}",
            task_id,
            repo_root.is_some()
        );
        Ok(())
    }

    fn after_task(
        &self,
        _task_id: &str,
        _context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    fn on_error(&self, task_id: &str, _context: &Context, error: &(dyn Error + Send + Sync)) {
        log::warn!("MergePrWorkflowHooks::on_error task={task_id} err={error}");
    }
}

/// Read the changeset from session_dir and extract the target branch name
/// when `branch_worktree_intent` is `WorkOnSelectedBranch`.
fn resolve_target_branch_from_changeset(context: &Context) -> Option<String> {
    let session_dir = context.get_sync::<PathBuf>("session_dir")?;
    let cs = read_changeset(&session_dir).ok()?;
    let wf = cs.workflow.as_ref()?;
    if wf.branch_worktree_intent == Some(BranchWorktreeIntent::WorkOnSelectedBranch) {
        let branch = wf.selected_branch_to_work_on.clone();
        if branch.is_some() {
            log::info!(
                "[merge-pr hooks] target branch from changeset: {:?}",
                branch
            );
        }
        branch
    } else {
        None
    }
}

const SCOPE: &str = "\n\n## Branch changes (deterministic scope)\n\n";

fn analyze_system_prompt(git_block: &str, target_branch: Option<&str>) -> String {
    let mut s = String::from(
        "You are assisting with the **merge-pr** workflow **analyze** step.\n\n\
         ## Task: Analyze merge feasibility\n\n\
         This is a **read-only** analysis phase. Fetch from **origin** and examine the \
         relationship between the current feature branch and the integration branch \
         (e.g. **origin/main** or **origin/master**).\n\n",
    );
    if let Some(branch) = target_branch {
        s.push_str(&format!(
            "**Target branch:** `{branch}` — this is the branch selected for this session. \
             If the working directory is not on this branch, check out or analyze `{branch}` \
             instead of the currently checked-out branch.\n\n"
        ));
    }
    s.push_str(
        "Determine:\n\
         1. Whether a merge is needed (is the branch already up to date?)\n\
         2. Whether the merge would be clean (fast-forward or no conflicts)\n\
         3. If there are conflicts, list the conflicting files and describe the nature of each conflict\n\
         4. Recommend whether this can be done automatically or needs manual conflict resolution\n\n\
         **Do NOT perform the actual merge.** Only analyze and report.\n",
    );
    s.push_str(SCOPE);
    s.push_str(git_block);
    s
}

fn sync_main_system_prompt(git_block: &str) -> String {
    let mut s = String::from(
        "You are assisting with the **merge-pr** workflow **sync-main** step.\n\n\
         ## Task: Sync with main\n\n\
         Merge the integration branch (e.g. **origin/main**) into the current feature branch \
         and resolve any merge conflicts. The analysis phase has already assessed the merge; \
         now perform the actual merge in this worktree.\n",
    );
    s.push_str(SCOPE);
    s.push_str(git_block);
    s
}

fn finalize_system_prompt() -> String {
    "You are assisting with the **merge-pr** workflow **finalize** step.\n\n\
     ## Task: Finalize\n\n\
     Submit the structured **merge-pr-report** with the outcome: sync strategy used, PR number \
     if applicable, merge commit SHA from the GitHub API or local push, or a clear skip reason \
     (e.g. no credentials, no open PR).\n"
        .to_string()
}

/// Supplemental guidance appended to merge-pr prompts when GitHub credentials are available (PRD).
#[must_use]
pub fn merge_pr_github_tools_awareness_line(has_github_token: bool) -> &'static str {
    if !has_github_token {
        log::debug!(
            "merge_pr_github_tools_awareness_line: has_github_token=false — returning empty awareness"
        );
        return "";
    }
    log::debug!(
        "merge_pr_github_tools_awareness_line: returning authenticated GitHub PR tools awareness"
    );
    MERGE_PR_GITHUB_TOOLS_AWARENESS_AUTHENTICATED
}

/// Static copy for merge-pr when `GITHUB_TOKEN` / `GH_TOKEN` is set (see [`merge_pr_github_tools_awareness_line`]).
const MERGE_PR_GITHUB_TOOLS_AWARENESS_AUTHENTICATED: &str = "When authenticated (**GITHUB_TOKEN** or **GH_TOKEN**), **tddy-tools** exposes GitHub pull request MCP tools (**github_create_pull_request**, **github_update_pull_request**) in addition to this workflow’s automated merge path—use them to open or update PR metadata without ad-hoc shell **curl**.";

#[must_use]
fn system_prompt_for_task(
    task_id: &str,
    git_block: &str,
    target_branch: Option<&str>,
) -> Option<String> {
    let mut base = match task_id {
        TASK_ANALYZE => Some(analyze_system_prompt(git_block, target_branch)),
        TASK_SYNC_MAIN => Some(sync_main_system_prompt(git_block)),
        TASK_FINALIZE => Some(finalize_system_prompt()),
        _ => None,
    }?;

    if github_env_token_present() {
        let line = merge_pr_github_tools_awareness_line(true);
        if !line.is_empty() {
            log::info!(
                "[merge-pr hooks] appending GitHub PR tools awareness to task_id={task_id} system prompt"
            );
            base.push_str("\n\n## GitHub PR tools (**tddy-tools**)\n\n");
            base.push_str(line);
        }
    } else {
        log::debug!(
            "[merge-pr hooks] no GitHub token in environment — omitting GitHub PR tools awareness"
        );
    }

    Some(base)
}

#[cfg(test)]
mod tests {
    use super::{
        analyze_system_prompt, finalize_system_prompt, sync_main_system_prompt, TASK_ANALYZE,
        TASK_FINALIZE, TASK_SYNC_MAIN,
    };

    #[test]
    fn analyze_prompt_mentions_read_only_and_conflicts() {
        let p = analyze_system_prompt("_diff_", None);
        assert!(p.contains("read-only") || p.contains("Read-only"));
        assert!(p.contains("conflict"));
        assert!(p.contains("## Branch changes (deterministic scope)"));
        assert!(p.ends_with("_diff_"));
    }

    #[test]
    fn analyze_prompt_includes_target_branch_when_set() {
        let p = analyze_system_prompt("_diff_", Some("feature/other"));
        assert!(
            p.contains("feature/other"),
            "analyze prompt must include the target branch; got: {p}"
        );
        assert!(p.contains("Target branch"));
    }

    #[test]
    fn analyze_prompt_omits_target_branch_section_when_none() {
        let p = analyze_system_prompt("_diff_", None);
        assert!(
            !p.contains("Target branch"),
            "no target-branch section when branch is None"
        );
    }

    #[test]
    fn sync_main_prompt_mentions_merge_and_scope() {
        let p = sync_main_system_prompt("_diff_");
        assert!(p.contains("Merge"));
        assert!(p.contains("## Branch changes (deterministic scope)"));
        assert!(p.ends_with("_diff_"));
    }

    #[test]
    fn finalize_prompt_mentions_merge_pr_report() {
        let p = finalize_system_prompt();
        assert!(p.contains("merge-pr-report"));
        assert!(p.contains("finalize"));
    }

    #[test]
    fn system_prompt_for_task_unknown_returns_none() {
        assert!(super::system_prompt_for_task("end", "x", None).is_none());
    }

    #[test]
    fn system_prompt_for_task_all_known_goals() {
        assert!(super::system_prompt_for_task(TASK_ANALYZE, "g", None).is_some());
        assert!(super::system_prompt_for_task(TASK_SYNC_MAIN, "g", None).is_some());
        assert!(super::system_prompt_for_task(TASK_FINALIZE, "g", None).is_some());
    }
}
