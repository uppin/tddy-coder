//! Hooks for [`super::ReviewRecipe`] — branch-diff context, elicitation relay (grill-me family).

use std::error::Error;
use std::sync::mpsc;

use tddy_core::backend::AgentOutputSink;
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::TaskResult;

use super::git_context::{
    format_diff_context_for_prompt, merge_base_commit_for_review, resolve_git_repo_root,
};
use super::prompt::{branch_review_system_prompt, inspect_system_prompt};
use super::{TASK_BRANCH_REVIEW, TASK_INSPECT};

/// Workflow hooks for **review** (`inspect` → `branch-review`).
#[derive(Debug)]
pub struct ReviewWorkflowHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl ReviewWorkflowHooks {
    pub fn new(event_tx: Option<mpsc::Sender<WorkflowEvent>>) -> Self {
        log::debug!("ReviewWorkflowHooks::new event_tx={}", event_tx.is_some());
        Self { event_tx }
    }
}

impl RunnerHooks for ReviewWorkflowHooks {
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
        log::debug!("[review hooks] before_task: {}", task_id);

        let working = context
            .get_sync::<std::path::PathBuf>("worktree_dir")
            .or_else(|| context.get_sync::<std::path::PathBuf>("output_dir"))
            .or_else(|| context.get_sync::<std::path::PathBuf>("session_dir"));

        let repo_root = working.as_ref().and_then(|p| resolve_git_repo_root(p));

        let git_block = if let Some(ref repo) = repo_root {
            let base = merge_base_commit_for_review(repo);
            log::debug!("[review hooks] repo={} merge_base={}", repo.display(), base);
            format_diff_context_for_prompt(repo, &base)
        } else {
            log::info!(
                "[review hooks] no git repo found from worktree_dir/output_dir/session_dir; diff context omitted"
            );
            "_Git repository not found from session/worktree paths; merge-base diff unavailable._\n"
                .to_string()
        };

        if let Some(prompt) = system_prompt_with_git_block(task_id, &git_block) {
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
            "[review hooks] before_task complete task_id={} repo_resolved={}",
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
        log::warn!("ReviewWorkflowHooks::on_error task={task_id} err={error}");
    }
}

/// Builds the full system prompt for `inspect` / `branch-review`, appending the git context block.
#[must_use]
pub(crate) fn system_prompt_with_git_block(task_id: &str, git_block: &str) -> Option<String> {
    const SCOPE: &str = "\n\n## Branch changes (deterministic scope)\n\n";
    match task_id {
        TASK_INSPECT => {
            let mut prompt = inspect_system_prompt();
            prompt.push_str(SCOPE);
            prompt.push_str(git_block);
            Some(prompt)
        }
        TASK_BRANCH_REVIEW => {
            let mut prompt = branch_review_system_prompt();
            prompt.push_str(SCOPE);
            prompt.push_str(git_block);
            Some(prompt)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{TASK_BRANCH_REVIEW, TASK_INSPECT};
    use super::system_prompt_with_git_block;

    #[test]
    fn system_prompt_with_git_block_inspect_includes_scope_and_placeholder() {
        let p = system_prompt_with_git_block(TASK_INSPECT, "_diff_").expect("inspect");
        assert!(
            p.contains("## Branch changes (deterministic scope)"),
            "expected scope header; got {}",
            p
        );
        assert!(p.ends_with("_diff_"), "git block must be appended last");
        assert!(
            p.contains("Inspect") || p.contains("inspect"),
            "inspect prompt expected; got {}",
            p
        );
    }

    #[test]
    fn system_prompt_with_git_block_branch_review_includes_scope() {
        let p = system_prompt_with_git_block(TASK_BRANCH_REVIEW, "x").expect("branch-review");
        assert!(p.contains("## Branch changes (deterministic scope)"));
        assert!(p.contains("branch-review") || p.contains("Branch review"));
    }

    #[test]
    fn system_prompt_with_git_block_unknown_task_returns_none() {
        assert!(system_prompt_with_git_block("end", "z").is_none());
    }
}
