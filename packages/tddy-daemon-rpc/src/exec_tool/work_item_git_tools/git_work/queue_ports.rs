//! The real git and GitHub behind the land and propagate engines (`#workqueue` 14/14): every git
//! operation runs in the worktree that holds the item's branch, and GitHub is reached as the
//! project's account.

use std::path::PathBuf;

use async_trait::async_trait;
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_projects::project_storage::ProjectData;
use tddy_work_queue::engine::{EngineError, LocalBranchState, QueueGit, QueueGithub};
use tddy_worktree_service::session_git_ops::{GitOp, PushIdentity};
use tddy_worktree_service::session_target::SessionTarget;

use super::super::super::work_queue_tools::WorkQueueGithub;
use super::{git, project_push_token, GitEnv};

/// One queue's git and GitHub, for the engines.
pub(crate) struct DaemonQueueWorld<'a> {
    env: &'a GitEnv,
    queue: WorkQueue,
    project: ProjectData,
    repo_root: PathBuf,
    remote: String,
    gh: Box<dyn WorkQueueGithub>,
}

fn failed(why: String) -> EngineError {
    EngineError(why)
}

impl<'a> DaemonQueueWorld<'a> {
    pub(crate) fn new(env: &'a GitEnv) -> Result<Self, String> {
        let project = env.project()?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        let remote = env.remote_name(&project, &repo_root);
        Ok(Self {
            queue: env.stack()?,
            gh: env.github_client()?,
            env,
            project,
            repo_root,
            remote,
        })
    }

    pub(crate) fn queue(&self) -> &WorkQueue {
        &self.queue
    }

    pub(crate) fn github(&self) -> &dyn WorkQueueGithub {
        &*self.gh
    }

    pub(crate) fn remote(&self) -> &str {
        &self.remote
    }

    pub(crate) fn project(&self) -> &ProjectData {
        &self.project
    }

    fn item_on(&self, branch: &str) -> Result<&WorkItem, EngineError> {
        self.queue
            .nodes
            .iter()
            .find(|item| item.branch.as_deref() == Some(branch))
            .ok_or_else(|| failed(format!("no work item owns branch '{branch}'")))
    }

    /// The worktree holding `branch`.
    fn worktree_of(&self, branch: &str) -> Result<PathBuf, EngineError> {
        self.env
            .worktree_for(self.item_on(branch)?, &self.repo_root)
            .map_err(failed)
    }

    fn remote_ref(&self, branch: &str) -> String {
        format!("{}/{branch}", self.remote)
    }

    fn git_in_repo(&self, args: &[&str]) -> Result<String, EngineError> {
        git(&self.repo_root, args).map_err(failed)
    }
}

#[async_trait]
impl QueueGit for DaemonQueueWorld<'_> {
    async fn fetch(&self) -> Result<(), EngineError> {
        self.git_in_repo(&["fetch", "--quiet", &self.remote])
            .map(drop)
    }

    async fn local_branch_state(&self, branch: &str) -> Result<LocalBranchState, EngineError> {
        let worktree = self.worktree_of(branch)?;
        if !self.env.dirty_paths(&worktree).map_err(failed)?.is_empty() {
            return Ok(LocalBranchState::Dirty);
        }
        let remote_ref = self.remote_ref(branch);
        if self
            .git_in_repo(&["rev-parse", "--verify", "--quiet", &remote_ref])
            .is_err()
        {
            return Ok(LocalBranchState::Unpublished);
        }
        let counts = self.git_in_repo(&[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{branch}...{remote_ref}"),
        ])?;
        let mut numbers = counts
            .split_whitespace()
            .map(|n| n.parse::<u32>().unwrap_or(0));
        let (ahead, behind) = (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0));
        Ok(match (ahead, behind) {
            (0, 0) => LocalBranchState::InSync,
            (_, 0) => LocalBranchState::Unpushed,
            (0, _) => LocalBranchState::Unpulled,
            _ => LocalBranchState::Diverged,
        })
    }

    async fn inbox_count(&self, branch: &str, onto: &str) -> Result<u32, EngineError> {
        let range = format!("{}..{}", self.remote_ref(branch), self.remote_ref(onto));
        self.git_in_repo(&["rev-list", "--count", &range])?
            .parse()
            .map_err(|e| failed(format!("git rev-list --count {range}: {e}")))
    }

    async fn predicted_conflicts(
        &self,
        branch: &str,
        onto: &str,
    ) -> Result<Vec<String>, EngineError> {
        let worktree = self.worktree_of(branch)?;
        super::node_ops::merge_tree_conflicts(&worktree, branch, &self.remote_ref(onto))
            .map_err(failed)
    }

    async fn tip(&self, branch: &str) -> Result<String, EngineError> {
        self.git_in_repo(&["rev-parse", &format!("refs/heads/{branch}")])
    }

    async fn rebase_onto(&self, branch: &str, onto: &str) -> Result<(), EngineError> {
        if let Some(session_id) = self.env.is_running(self.item_on(branch)?) {
            return Err(failed(format!(
                "session {session_id} is running: stop it before its branch is rebased"
            )));
        }
        let base_ref = self.remote_ref(onto);
        let worktree = self.worktree_of(branch)?;
        // The fork point is read, never invented: rebasing from a guessed base replays the wrong
        // commits.
        let old_base =
            tddy_work_queue::git_ops::merge_base(&worktree, branch, &base_ref).map_err(|e| {
                failed(format!(
                    "the fork point of {branch} on {onto} is unknown: {e}"
                ))
            })?;
        tddy_work_queue::git_ops::rebase_onto(&self.repo_root, &base_ref, &old_base, branch)
            .map_err(|e| failed(format!("rebase of {branch} onto {onto} failed: {e}")))
    }

    async fn push_with_lease(&self, branch: &str, _expected_sha: &str) -> Result<(), EngineError> {
        let item = self.item_on(branch)?;
        let worktree = self.worktree_of(branch)?;
        // Pushed as the project's account or not at all — never with whatever credentials the
        // host happens to hold.
        let token = project_push_token(
            &self.project,
            &self.env.identity,
            &self.env.github_cli,
            &self.env.os_user,
        )
        .map(Some)
        .map_err(|e| {
            failed(format!(
                "{branch} is not pushed: the project's account: {e}"
            ))
        })?;
        let identity = PushIdentity {
            env: Vec::new(),
            remote_name: Some(self.remote.clone()),
            account_token: token,
        };
        let target = SessionTarget {
            session_id: item.session_id.clone().unwrap_or_default(),
            project_id: self.env.project_id.clone(),
            worktree: Some(worktree),
            branch: branch.to_string(),
            base_ref: self.remote_ref(branch),
            is_active: false,
            codebase: None,
        };
        tddy_worktree_service::session_git_ops::run_git_op(
            &target,
            GitOp::Push {
                force_with_lease: true,
            },
            &identity,
        )
        .map(drop)
        .map_err(|e| {
            failed(format!(
                "force-push of {branch} to {} failed: {e}",
                self.remote
            ))
        })
    }
}

#[async_trait]
impl QueueGithub for DaemonQueueWorld<'_> {
    async fn merge(&self, number: u64) -> Result<String, EngineError> {
        self.gh
            .merge_pr(number)
            .map_err(|e| failed(format!("merge of #{number} failed: {e}")))
    }

    async fn retarget(&self, number: u64, base: &str) -> Result<(), EngineError> {
        self.gh
            .patch_pr_base(number, base)
            .map_err(|e| failed(format!("retargeting #{number} to {base} failed: {e}")))
    }
}
