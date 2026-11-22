use serde_json::Value;

use tddy_github::pr_api::owner_repo_from_remote_url;
use tddy_worktree_service::session_git_ops::GitOp;

use tddy_worktree_service::session_git_ops::PushIdentity;

use tddy_core::changeset::GithubPrStatus;

use tddy_worktree_service::session_target::SessionTarget;

use tddy_core::changeset::WorkItem;

use super::super::work_queue_tools::WorkQueueGithub;

use tddy_projects::project_storage::find_project;

use tddy_core::changeset::WorkQueue;

use tddy_projects::project_storage::ProjectData;

use std::path::Path;

use super::super::work_queue_tools::WorkQueueGithubClients;

use std::sync::Arc;

use tddy_host_service::GithubCli;

use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;

use std::path::PathBuf;

use tddy_core::work_queue_store::WorkQueueStore;

pub(crate) enum GitWork {
    Merge {
        node_id: String,
    },
    Close {
        node_id: String,
    },
    Repoint {
        node_id: String,
        new_base: Option<String>,
    },
    SetParents {
        node_id: String,
        parents: Vec<String>,
    },
    ResolveConflicts {
        node_id: String,
    },
    Propagate {
        item_id: String,
        force: bool,
    },
    Land {
        from_wave: u32,
        to_wave: u32,
        held_up: std::collections::HashMap<String, (String, String)>,
    },
}

pub(crate) struct GitEnv {
    pub(crate) store: WorkQueueStore,
    pub(crate) projects_dir: PathBuf,
    pub(crate) project_id: String,
    pub(crate) queue_id: String,
    pub(crate) records: Vec<tddy_worktree_service::session_target::TargetRecord>,
    pub(crate) os_user: String,
    pub(crate) identity: SessionAccountAccess,
    pub(crate) github_cli: GithubCli,
    pub(crate) clients: Arc<dyn WorkQueueGithubClients>,
}

pub(super) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git {} in {}: {e}", args.join(" "), dir.display()))?;
    if !output.status.success() {
        return Err(format!(
            "git {} in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(super) fn project_token(
    project: &ProjectData,
    identity: &SessionAccountAccess,
    github_cli: &GithubCli,
    os_user: &str,
) -> Result<String, String> {
    if project.local_gh {
        tddy_agent_launch::session_acting_identity::local_gh_github_token(
            github_cli,
            os_user,
            &project.git_url,
        )
    } else {
        identity
            .acting_identity(&project.accounts)
            .map(|acting| acting.token)
            .map_err(|refusal| refusal.to_string())
    }
}

impl GitEnv {
    pub(crate) fn stack(&self) -> Result<WorkQueue, String> {
        self.store
            .read(&self.project_id, &self.queue_id)
            .map(|record| record.stack)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn project(&self) -> Result<ProjectData, String> {
        find_project(&self.projects_dir, &self.project_id)
            .map_err(|e| format!("project {} could not be read: {e:#}", self.project_id))?
            .ok_or_else(|| format!("project {} is not registered on this host", self.project_id))
    }

    pub(crate) fn github_client(&self) -> Result<Box<dyn WorkQueueGithub>, String> {
        let project = self.project()?;
        let repo = owner_repo_from_remote_url(&project.git_url).ok_or_else(|| {
            format!(
                "project {} names no GitHub repository: its git url is {:?}",
                project.project_id, project.git_url
            )
        })?;
        let token = project_token(&project, &self.identity, &self.github_cli, &self.os_user)?;
        Ok(self.clients.client_for(&repo, &token))
    }

    pub(crate) fn remote_name(&self, project: &ProjectData, repo_root: &Path) -> String {
        if let Some(remote) = project
            .remote_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            return remote.to_string();
        }
        let remotes = git(repo_root, &["remote"]).unwrap_or_default();
        let names: Vec<&str> = remotes
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        if names.contains(&"origin") {
            "origin".to_string()
        } else {
            names
                .first()
                .map(|name| name.to_string())
                .unwrap_or_else(|| "origin".to_string())
        }
    }

    pub(crate) fn default_branch(&self, project: &ProjectData) -> String {
        let reference = tddy_projects::project_storage::effective_integration_base_ref_for_project(
            &self.projects_dir,
            &self.project_id,
        )
        .ok()
        .or_else(|| project.main_branch_ref.clone());
        match reference
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(reference) => reference
                .rsplit_once('/')
                .map(|(_, branch)| branch.to_string())
                .unwrap_or_else(|| reference.to_string()),
            None => "main".to_string(),
        }
    }

    pub(crate) fn target_for(&self, node: &WorkItem) -> Option<SessionTarget> {
        let session_id = node.session_id.as_deref()?;
        self.records
            .iter()
            .find(|record| record.session_id == session_id)
            .map(|record| SessionTarget {
                session_id: record.session_id.clone(),
                project_id: record.project_id.clone(),
                worktree: record.worktree.clone(),
                branch: record.branch.clone(),
                base_ref: record.base_ref.clone(),
                is_active: record.is_active,
                codebase: record.codebase.clone(),
            })
    }

    pub(crate) fn is_running(&self, node: &WorkItem) -> Option<String> {
        let target = self.target_for(node)?;
        target.is_active.then_some(target.session_id.clone())
    }

    pub(crate) fn worktree_for(
        &self,
        node: &WorkItem,
        repo_root: &Path,
    ) -> Result<PathBuf, String> {
        let branch = node
            .branch
            .as_deref()
            .ok_or_else(|| format!("node '{}' owns no branch yet", node.node_id))?;
        if let Some(worktree) = self.target_for(node).and_then(|target| target.worktree) {
            if git(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"])
                .ok()
                .as_deref()
                == Some(branch)
            {
                return Ok(worktree);
            }
        }
        let listing = git(repo_root, &["worktree", "list", "--porcelain"])?;
        let mut current: Option<PathBuf> = None;
        for line in listing.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                current = Some(PathBuf::from(path.trim()));
            } else if line
                .strip_prefix("branch ")
                .is_some_and(|reference| reference.trim() == format!("refs/heads/{branch}"))
            {
                if let Some(path) = current.clone() {
                    return Ok(path);
                }
            }
        }
        Err(format!(
            "branch '{branch}' has no worktree checked out, so there is nowhere to run git for node '{}' — start or resume the session that owns it first",
            node.node_id
        ))
    }

    pub(crate) fn dirty_paths(&self, worktree: &Path) -> Result<Vec<String>, String> {
        let output = git(worktree, &["status", "--porcelain"])?;
        Ok(output
            .lines()
            .filter_map(|line| {
                let path = line.get(3..)?.trim();
                if path.is_empty() {
                    None
                } else {
                    Some(
                        path.rsplit_once(" -> ")
                            .map(|(_, new)| new)
                            .unwrap_or(path)
                            .trim_matches('"')
                            .to_string(),
                    )
                }
            })
            .collect())
    }

    pub(crate) fn refuse_dirty_or_running(
        &self,
        node: &WorkItem,
        worktree: &Path,
    ) -> Result<(), String> {
        let dirty = self.dirty_paths(worktree)?;
        if !dirty.is_empty() {
            return Err(format!(
                "the worktree for node '{}' has uncommitted changes to {} — commit or stash them before rebasing",
                node.node_id,
                dirty.join(", ")
            ));
        }
        if let Some(session_id) = self.is_running(node) {
            return Err(format!(
                "session {session_id} is running: stop it before its branch is rebased"
            ));
        }
        Ok(())
    }

    /// (base branch name for GitHub, ref to rebase onto / merge against)
    pub(crate) fn effective_base(
        &self,
        stack: &WorkQueue,
        node_id: &str,
        remote: &str,
        default_branch: &str,
    ) -> (String, String) {
        if let Some(node) = stack.node(node_id) {
            for parent_id in &node.parents {
                if let Some(parent) = stack.node(parent_id) {
                    if !parent.is_skipped() {
                        if let Some(branch) = parent.branch.as_deref() {
                            return (branch.to_string(), branch.to_string());
                        }
                    }
                }
            }
        }
        (
            default_branch.to_string(),
            format!("{remote}/{default_branch}"),
        )
    }

    pub(crate) fn set_phase(&self, node_id: &str, phase: &str) -> Result<(), String> {
        self.store
            .update(&self.project_id, &self.queue_id, |stack| {
                if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
                    match node.pr_status.as_mut() {
                        Some(status) => status.phase = phase.to_string(),
                        None => {
                            node.pr_status = Some(GithubPrStatus {
                                phase: phase.to_string(),
                                url: None,
                                error: None,
                            })
                        }
                    }
                }
            })
            .map(drop)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn pull_number(
        &self,
        stack: &WorkQueue,
        node: &WorkItem,
        gh: &dyn WorkQueueGithub,
    ) -> Result<u64, String> {
        if let Some(branch) = node.branch.as_deref() {
            if let Some(pr) = gh
                .get_open_pr(branch)
                .map_err(|e| format!("get_open_pr failed: {e}"))?
            {
                return Ok(pr.number);
            }
        }
        tddy_work_queue::pr_insight::pull_number_for_node(stack, &node.node_id)
    }

    pub(crate) fn realign(
        &self,
        node: &WorkItem,
        base_branch: &str,
        base_ref: &str,
    ) -> Result<(), String> {
        let project = self.project()?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        let remote = self.remote_name(&project, &repo_root);
        let branch = node
            .branch
            .as_deref()
            .ok_or_else(|| format!("node '{}' owns no branch yet", node.node_id))?;
        let worktree = self.worktree_for(node, &repo_root)?;
        self.refuse_dirty_or_running(node, &worktree)?;
        let old_base = tddy_work_queue::git_ops::merge_base(&worktree, branch, base_ref)
            .unwrap_or_else(|_| base_ref.to_string());
        tddy_work_queue::git_ops::rebase_onto(&repo_root, base_ref, &old_base, branch)
            .map_err(|e| format!("rebase of {branch} onto {base_branch} failed: {e}"))?;
        let token = project_token(&project, &self.identity, &self.github_cli, &self.os_user).ok();
        let identity = PushIdentity {
            env: Vec::new(),
            remote_name: Some(remote.clone()),
            account_token: token,
        };
        let target = SessionTarget {
            session_id: node.session_id.clone().unwrap_or_default(),
            project_id: self.project_id.clone(),
            worktree: Some(worktree.clone()),
            branch: branch.to_string(),
            base_ref: base_ref.to_string(),
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
        .map_err(|e| format!("force-push of {branch} to {remote} failed: {e}"))?;
        let gh = self.github_client()?;
        if let Some(pr) = gh
            .get_open_pr(branch)
            .map_err(|e| format!("get_open_pr failed: {e}"))?
        {
            gh.patch_pr_base(pr.number, base_branch)
                .map_err(|e| format!("patch_pr_base failed: {e}"))?;
        }
        Ok(())
    }
}

pub(super) fn node_of<'a>(stack: &'a WorkQueue, node_id: &str) -> Result<&'a WorkItem, String> {
    stack
        .node(node_id)
        .ok_or_else(|| format!("node '{node_id}' not found"))
}

pub(crate) fn run_git_work(env: &GitEnv, work: GitWork) -> Result<Value, String> {
    match work {
        GitWork::Merge { node_id } => node_ops::merge_node(env, &node_id),
        GitWork::Close { node_id } => node_ops::close_node(env, &node_id),
        GitWork::Repoint { node_id, new_base } => {
            node_ops::repoint_node(env, &node_id, new_base.as_deref())
        }
        GitWork::SetParents { node_id, parents } => node_ops::set_parents(env, &node_id, &parents),
        GitWork::ResolveConflicts { node_id } => node_ops::resolve_conflicts(env, &node_id),
        GitWork::Propagate { item_id, force } => land_ops::propagate_tool(env, &item_id, force),
        GitWork::Land {
            from_wave,
            to_wave,
            held_up,
        } => land_ops::land_waves(env, from_wave, to_wave, &held_up),
    }
}

/// Beside the queue, in its owning session's directory (`#workqueue` 5/14).
pub(super) fn journal_path(env: &GitEnv) -> Result<PathBuf, String> {
    env.store
        .op_journal_path(&env.queue_id)
        .map_err(|e| e.to_string())
}

pub(super) fn write_journal(
    env: &GitEnv,
    journal: &tddy_work_queue::op_journal::WorkQueueOpJournal,
) -> Result<(), String> {
    let path = journal_path(env)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(journal).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

pub(crate) mod land_ops;
mod node_ops;
mod queue_ports;
