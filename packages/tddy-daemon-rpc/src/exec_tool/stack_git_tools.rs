//! The PR-stack git tools ([`tddy_tool_engine::stack_git_tools`]) answered on this daemon
//! (`#orchestrators` 10/11).
//!
//! Routed from `run_locally` beside the plan tools ([`super::stack_tools`]) and classified the same
//! way (`stack_caller_of`): only an orchestrator-role caller is served, and a call naming another
//! host with `daemon_instance_id` is forwarded whole to that host, one hop.
//!
//! Every git step runs **here, in the node's own worktree** — the worktree of the project's
//! repository that holds the node's branch — never in the caller's checkout:
//!
//! - a rebase (`pr_repoint`, `pr_set_parents`) is refused, untouched, while that worktree has
//!   uncommitted changes or the node's session is running, then force-pushes with lease to the
//!   project's remote with the project account's token
//!   ([`tddy_worktree_service::session_git_ops::PushIdentity`]);
//! - `pr_resolve_conflicts` finds the conflicting paths with `git merge-tree --write-tree`,
//!   leaving the worktree exactly as it was, and with `delegate` hands them to the node's session
//!   through `SendSessionMessage` (running) or `ResumeSession` with the message (stopped) —
//!   `#orchestrators` 4/11's turn port;
//! - `pr_spawn_child` starts the node's session through the daemon's start path, its stack parent
//!   the caller, and binds the node to the started session;
//! - `pr_merge` / `pr_close` reach GitHub as the project's account.
//!
//! A merge keeps its crash-safety journal: beside the stack
//! (`<projects>/<project_id>/stacks/<stack_id>.op.json`) for an operation an orchestrator started.
//!
//! Feature doc: docs/ft/coder/pr-stacking.md § Merge and repoint.
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-git-tools.md.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tddy_core::changeset::{GithubPrStatus, PrInternalStatus, Stack, StackNode};
use tddy_core::project_stack_store::ProjectStackStore;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_github::pr_api::owner_repo_from_remote_url;
use tddy_host_service::GithubCli;
use tddy_projects::project_storage::{find_project, ProjectData};
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_tool_engine::project_home_tools::ProjectHomePort;
use tddy_tool_engine::project_home_turn_tools::ProjectHomeTurnPort;
use tddy_tool_engine::stack_git_tools::{
    execute_stack_git_tool, is_stack_git_tool_offered_to, StackGitOp, StackGitToolPort,
};
use tddy_tool_engine::stack_tools::{StackCaller, StackToolPort};
use tddy_tool_engine::start_ledger::StartState;
use tddy_tool_engine::start_session_args::StartSessionArgs;
use tddy_tool_engine::ToolOutcome;
use tddy_worktree_service::session_git_ops::{GitOp, PushIdentity};
use tddy_worktree_service::session_target::{resolve_session_target, HomeCaller, SessionTarget};

use super::project_home::caller::record_call;
use super::project_home::DaemonProjectHome;
use super::stack_tools::{stack_caller_of, DaemonStackTools, StackGithub, StackGithubClients};
use super::ExecToolRpcHandler;

impl ExecToolRpcHandler {
    /// Serve `req`'s stack git tool: classify the caller, refuse one that is not an orchestrator,
    /// forward a call naming another host, else run it on this host's stack and record the call.
    pub(super) async fn run_stack_git_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        let tool = &req.tool_name;
        let caller = stack_caller_of(sessions_base, &req.session_id)?.ok_or_else(|| {
            Status::permission_denied(format!(
                "tool {tool} is served only to an orchestrator session"
            ))
        })?;
        if !is_stack_git_tool_offered_to(&caller.role, tool) {
            return Err(Status::permission_denied(format!(
                "tool {tool} is not offered to a session bound to a node of a stack: the stack \
                 git tools are served to an orchestrator alone"
            )));
        }
        let response = match self.served_on_target_host(req).await? {
            Some(answered) => answered,
            None => {
                let os_user = self.os_user_of(&req.session_token)?;
                let port = self.stack_git_tool_port(req, os_user, sessions_base)?;
                response_of(execute_stack_git_tool(&port, &caller, tool, &req.args_json).await)
            }
        };
        record_call(sessions_base, req, &response);
        Ok(response)
    }

    /// Serve a stack git tool called by a session another host holds — forwarded here by that
    /// host — once the daemon that owns the session has confirmed it holds an orchestrator role.
    /// Such a caller owns no stack here, so a call must name the stack it acts on.
    pub(super) async fn serve_stack_git_tool_for_caller_held_elsewhere(
        &self,
        req: &ExecuteToolRequest,
        caller: StackCaller,
        os_user: String,
        sessions_base: &Path,
    ) -> ExecuteToolResponse {
        match self.stack_git_tool_port(req, os_user, sessions_base) {
            Ok(port) => response_of(
                execute_stack_git_tool(&port, &caller, &req.tool_name, &req.args_json).await,
            ),
            Err(status) => ExecuteToolResponse {
                result_json: String::new(),
                is_error: true,
                error_message: status.message().to_string(),
                job_id: String::new(),
                job_running: false,
            },
        }
    }

    /// [`DaemonStackGitTools`] over this host for `req`'s caller.
    fn stack_git_tool_port(
        &self,
        req: &ExecuteToolRequest,
        os_user: String,
        sessions_base: &Path,
    ) -> Result<DaemonStackGitTools, Status> {
        let stacks = self.stack_tool_port(req, os_user.clone(), sessions_base);
        let local_id =
            tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(&self.config);
        let home = self.project_home_port(req, os_user.clone(), sessions_base, None, local_id)?;
        let store = ProjectStackStore::new(sessions_base.join("projects"));
        Ok(DaemonStackGitTools {
            stacks,
            home,
            store: store.clone(),
            sessions_base: sessions_base.to_path_buf(),
            projects_dir: store.projects_dir().to_path_buf(),
            os_user,
            identity: SessionAccountAccess::new(
                self.project_home_host.credential_vaults.clone(),
                self.user_resolver.clone(),
                &req.session_token,
            ),
            github_cli: self.project_home_host.github_cli.clone(),
            github: self
                .project_home_host
                .stack_github
                .get()
                .cloned()
                .unwrap_or_else(|| Arc::new(super::stack_tools::RealStackGithubClients)),
        })
    }
}

/// The answer a tool's outcome is on the wire.
fn response_of(outcome: ToolOutcome) -> ExecuteToolResponse {
    ExecuteToolResponse {
        result_json: outcome.result_json,
        is_error: outcome.is_error,
        error_message: outcome.error_message,
        job_id: outcome.job_id,
        job_running: outcome.job_running,
    }
}

impl DaemonStackGitTools {
    async fn run_blocking(
        &self,
        caller: &StackCaller,
        stack_id: &str,
        work: GitWork,
    ) -> Result<Value, String> {
        let records = self
            .home
            .target_records()
            .await
            .map_err(|e| format!("{e:#}"))?;
        let env = GitEnv {
            store: self.store.clone(),
            projects_dir: self.projects_dir.clone(),
            project_id: caller.project_id.clone(),
            stack_id: stack_id.to_string(),
            records,
            os_user: self.os_user.clone(),
            identity: self.identity.clone(),
            github_cli: self.github_cli.clone(),
            clients: Arc::clone(&self.github),
        };
        tokio::task::spawn_blocking(move || run_git_work(&env, work))
            .await
            .map_err(|e| format!("the stack git operation failed to finish: {e}"))?
    }

    async fn delegate_conflicts(
        &self,
        caller: &StackCaller,
        stack_id: &str,
        node_id: &str,
        paths: &[String],
    ) -> Result<(), String> {
        let record = self
            .store
            .read(&caller.project_id, stack_id)
            .map_err(|e| e.to_string())?;
        let node = record
            .stack
            .node(node_id)
            .ok_or_else(|| format!("node '{node_id}' not found"))?;
        let session_id = node
            .session_id
            .clone()
            .ok_or_else(|| format!("node '{node_id}' has no session to resolve its conflicts"))?;
        let records = self
            .home
            .target_records()
            .await
            .map_err(|e| format!("{e:#}"))?;
        let home = HomeCaller {
            session_id: caller.session_id.clone(),
            project_id: caller.project_id.clone(),
        };
        // A node bound to the caller itself has no other session to message; the caller is the
        // session that would resolve them, so there is nothing to delegate.
        if session_id == caller.session_id {
            return Ok(());
        }
        let target =
            resolve_session_target(&records, &home, &session_id).map_err(|e| e.to_string())?;
        let message = format!(
            "Your branch conflicts with its base in: {}. Resolve the conflicts in those files, \
             commit the resolution, and push your branch.",
            paths.join(", ")
        );
        if target.is_active {
            self.home
                .send_session_message(&target, &message)
                .await
                .map(|_| ())
        } else {
            self.home
                .resume_session(&target, Some(&message))
                .await
                .map(|_| ())
        }
    }

    async fn spawn_child(
        &self,
        caller: &StackCaller,
        stack_id: &str,
        node_id: &str,
        session_type: Option<String>,
        model: Option<String>,
    ) -> Result<Value, String> {
        let record = self
            .store
            .read(&caller.project_id, stack_id)
            .map_err(|e| e.to_string())?;
        let node = record
            .stack
            .node(node_id)
            .cloned()
            .ok_or_else(|| format!("node '{node_id}' not found"))?;
        if let Some(branch) = node.branch.as_deref() {
            return Err(format!("node '{node_id}' already owns branch '{branch}'"));
        }
        let suggestion = node
            .branch_suggestion
            .clone()
            .ok_or_else(|| format!("node '{node_id}' has no branch_suggestion to create"))?;
        // The caller's own type and model are the defaults; a caller held on another host has
        // no record here to read them from, so an unnamed model then stays the start's default.
        let caller_dir = unified_session_dir_path(&self.sessions_base, &caller.session_id);
        let caller_meta = tddy_core::read_session_metadata(&caller_dir).ok();
        let session_type = session_type
            .or_else(|| {
                caller_meta
                    .as_ref()
                    .and_then(|meta| meta.session_type.clone())
            })
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "pr_spawn_child: no session_type named and the caller records none".to_string()
            })?;
        let model = model
            .or_else(|| caller_meta.as_ref().and_then(|meta| meta.model.clone()))
            .unwrap_or_default();
        let brief = if node.description.trim().is_empty() {
            node.title.clone()
        } else {
            format!("{}\n\n{}", node.title, node.description)
        };
        let args = StartSessionArgs {
            session_type,
            model,
            stack_parent: caller.session_id.clone(),
            stack_parent_daemon_instance_id: self.stacks.daemon_instance_id(),
            stack_node_id: node_id.to_string(),
            new_branch_name: suggestion.clone(),
            branch_worktree_intent: "new_branch_from_base".to_string(),
            initial_prompt: brief,
            ..StartSessionArgs::default()
        };
        let handle = self
            .home
            .start_session(&caller.project_id, &caller.session_id, args)
            .await
            .map_err(|e| format!("{e:#}"))?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        let child = loop {
            let started = self
                .home
                .get_session_start(&caller.session_id, &handle.start_id)
                .await
                .map_err(|e| format!("{e:#}"))?;
            match started.state {
                StartState::Succeeded => {
                    break started.session_id.ok_or_else(|| {
                        "the start succeeded without naming its session".to_string()
                    })?;
                }
                StartState::Failed | StartState::Cancelled => {
                    return Err(format!(
                        "starting node '{node_id}' failed: {}",
                        started
                            .error
                            .as_deref()
                            .unwrap_or("the start did not succeed")
                    ));
                }
                StartState::Running => {
                    if tokio::time::Instant::now() >= deadline {
                        return Err(format!(
                            "starting node '{node_id}' did not finish within 120s"
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        };
        let branch = suggestion;
        let child_for_store = child.clone();
        self.store
            .update(&caller.project_id, stack_id, |stack| {
                if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
                    node.session_id = Some(child_for_store.clone());
                    node.branch = Some(branch.clone());
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(json!({ "session_id": child }))
    }
}

/// [`StackGitToolPort`] over this host's stacks and the nodes' worktrees, for one caller: the plan
/// tools' port for the store and GitHub, and the project-home port for the node's session (its
/// target, its turn, its resume, a start).
pub(super) struct DaemonStackGitTools {
    pub(super) stacks: DaemonStackTools,
    pub(super) home: DaemonProjectHome,
    store: ProjectStackStore,
    sessions_base: PathBuf,
    projects_dir: PathBuf,
    os_user: String,
    identity: SessionAccountAccess,
    github_cli: GithubCli,
    github: Arc<dyn StackGithubClients>,
}

#[async_trait]
impl StackGitToolPort for DaemonStackGitTools {
    async fn run_stack_git_op(
        &self,
        caller: &StackCaller,
        stack_id: &str,
        op: StackGitOp,
    ) -> Result<Value, String> {
        match op {
            StackGitOp::SpawnChild {
                node_id,
                session_type,
                model,
            } => {
                self.spawn_child(caller, stack_id, &node_id, session_type, model)
                    .await
            }
            StackGitOp::ResolveConflicts { node_id, delegate } => {
                let answer = self
                    .run_blocking(
                        caller,
                        stack_id,
                        GitWork::ResolveConflicts {
                            node_id: node_id.clone(),
                        },
                    )
                    .await?;
                if delegate {
                    if let Some(conflicts) = answer["conflicts"]
                        .as_array()
                        .filter(|paths| !paths.is_empty())
                    {
                        let paths: Vec<String> = conflicts
                            .iter()
                            .filter_map(|path| path.as_str().map(str::to_string))
                            .collect();
                        self.delegate_conflicts(caller, stack_id, &node_id, &paths)
                            .await?;
                    }
                }
                Ok(answer)
            }
            StackGitOp::Merge { node_id } => {
                self.run_blocking(caller, stack_id, GitWork::Merge { node_id })
                    .await
            }
            StackGitOp::Close { node_id } => {
                self.run_blocking(caller, stack_id, GitWork::Close { node_id })
                    .await
            }
            StackGitOp::Repoint { node_id, new_base } => {
                self.run_blocking(caller, stack_id, GitWork::Repoint { node_id, new_base })
                    .await
            }
            StackGitOp::SetParents { node_id, parents } => {
                self.run_blocking(caller, stack_id, GitWork::SetParents { node_id, parents })
                    .await
            }
        }
    }
}

enum GitWork {
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
}

struct GitEnv {
    store: ProjectStackStore,
    projects_dir: PathBuf,
    project_id: String,
    stack_id: String,
    records: Vec<tddy_worktree_service::session_target::TargetRecord>,
    os_user: String,
    identity: SessionAccountAccess,
    github_cli: GithubCli,
    clients: Arc<dyn StackGithubClients>,
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
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

fn project_token(
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
    fn stack(&self) -> Result<Stack, String> {
        self.store
            .read(&self.project_id, &self.stack_id)
            .map(|record| record.stack)
            .map_err(|e| e.to_string())
    }

    fn project(&self) -> Result<ProjectData, String> {
        find_project(&self.projects_dir, &self.project_id)
            .map_err(|e| format!("project {} could not be read: {e:#}", self.project_id))?
            .ok_or_else(|| format!("project {} is not registered on this host", self.project_id))
    }

    fn github_client(&self) -> Result<Box<dyn StackGithub>, String> {
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

    fn remote_name(&self, project: &ProjectData, repo_root: &Path) -> String {
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

    fn default_branch(&self, project: &ProjectData) -> String {
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

    fn target_for(&self, node: &StackNode) -> Option<SessionTarget> {
        let session_id = node.session_id.as_deref()?;
        self.records
            .iter()
            .find(|record| record.session_id == session_id)
            .map(|record| SessionTarget {
                session_id: record.session_id.clone(),
                project_id: record.project_id.clone(),
                worktree: record.worktree.clone().unwrap_or_default(),
                branch: record.branch.clone(),
                base_ref: record.base_ref.clone(),
                is_active: record.is_active,
            })
    }

    fn is_running(&self, node: &StackNode) -> Option<String> {
        let target = self.target_for(node)?;
        target.is_active.then_some(target.session_id.clone())
    }

    fn worktree_for(&self, node: &StackNode, repo_root: &Path) -> Result<PathBuf, String> {
        let branch = node
            .branch
            .as_deref()
            .ok_or_else(|| format!("node '{}' owns no branch yet", node.node_id))?;
        if let Some(target) = self.target_for(node) {
            if !target.worktree.as_os_str().is_empty()
                && git(&target.worktree, &["rev-parse", "--abbrev-ref", "HEAD"])
                    .ok()
                    .as_deref()
                    == Some(branch)
            {
                return Ok(target.worktree);
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

    fn dirty_paths(&self, worktree: &Path) -> Result<Vec<String>, String> {
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

    fn refuse_dirty_or_running(&self, node: &StackNode, worktree: &Path) -> Result<(), String> {
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
    fn effective_base(
        &self,
        stack: &Stack,
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

    fn set_phase(&self, node_id: &str, phase: &str) -> Result<(), String> {
        self.store
            .update(&self.project_id, &self.stack_id, |stack| {
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

    fn pull_number(
        &self,
        stack: &Stack,
        node: &StackNode,
        gh: &dyn StackGithub,
    ) -> Result<u64, String> {
        if let Some(branch) = node.branch.as_deref() {
            if let Some(pr) = gh
                .get_open_pr(branch)
                .map_err(|e| format!("get_open_pr failed: {e}"))?
            {
                return Ok(pr.number);
            }
        }
        tddy_pr_stack::pr_insight::pull_number_for_node(stack, &node.node_id)
    }

    fn realign(&self, node: &StackNode, base_branch: &str, base_ref: &str) -> Result<(), String> {
        let project = self.project()?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        let remote = self.remote_name(&project, &repo_root);
        let branch = node
            .branch
            .as_deref()
            .ok_or_else(|| format!("node '{}' owns no branch yet", node.node_id))?;
        let worktree = self.worktree_for(node, &repo_root)?;
        self.refuse_dirty_or_running(node, &worktree)?;
        let old_base = tddy_pr_stack::git_ops::merge_base(&worktree, branch, base_ref)
            .unwrap_or_else(|_| base_ref.to_string());
        tddy_pr_stack::git_ops::rebase_onto(&repo_root, base_ref, &old_base, branch)
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
            worktree: worktree.clone(),
            branch: branch.to_string(),
            base_ref: base_ref.to_string(),
            is_active: false,
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

fn node_of<'a>(stack: &'a Stack, node_id: &str) -> Result<&'a StackNode, String> {
    stack
        .node(node_id)
        .ok_or_else(|| format!("node '{node_id}' not found"))
}

fn run_git_work(env: &GitEnv, work: GitWork) -> Result<Value, String> {
    match work {
        GitWork::Merge { node_id } => merge_node(env, &node_id),
        GitWork::Close { node_id } => close_node(env, &node_id),
        GitWork::Repoint { node_id, new_base } => repoint_node(env, &node_id, new_base.as_deref()),
        GitWork::SetParents { node_id, parents } => set_parents(env, &node_id, &parents),
        GitWork::ResolveConflicts { node_id } => resolve_conflicts(env, &node_id),
    }
}

fn journal_path(env: &GitEnv) -> PathBuf {
    env.projects_dir
        .join(&env.project_id)
        .join("stacks")
        .join(format!("{}.op.json", env.stack_id))
}

fn write_journal(
    env: &GitEnv,
    journal: &tddy_workflow_recipes::orchestrate_pr_stack::StackOpJournal,
) -> Result<(), String> {
    let path = journal_path(env);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(journal).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

fn merge_node(env: &GitEnv, node_id: &str) -> Result<Value, String> {
    use tddy_workflow_recipes::orchestrate_pr_stack::{MergePhase, StackOpJournal};
    let stack = env.stack()?;
    let node = node_of(&stack, node_id)?.clone();
    let gh = env.github_client()?;
    let number = env.pull_number(&stack, &node, &*gh)?;
    let dependents: Vec<String> = stack
        .nodes
        .iter()
        .filter(|candidate| candidate.parents.iter().any(|parent| parent == node_id))
        .map(|candidate| candidate.node_id.clone())
        .collect();
    let mut journal = StackOpJournal {
        op_id: uuid::Uuid::new_v4().to_string(),
        merged_node_id: node_id.to_string(),
        merge_phase: MergePhase::Planned,
        dependents,
    };
    write_journal(env, &journal)?;
    let sha = gh.merge_pr(number).map_err(|e| format!("merge pr: {e}"))?;
    journal.merge_phase = MergePhase::PrMerged { sha: sha.clone() };
    write_journal(env, &journal)?;
    env.set_phase(node_id, "merged")?;
    let _ = std::fs::remove_file(journal_path(env));
    Ok(json!({ "merged": true, "sha": sha }))
}

fn close_node(env: &GitEnv, node_id: &str) -> Result<Value, String> {
    let stack = env.stack()?;
    let node = node_of(&stack, node_id)?.clone();
    let gh = env.github_client()?;
    let number = env.pull_number(&stack, &node, &*gh)?;
    gh.close_pr(number).map_err(|e| format!("close pr: {e}"))?;
    env.set_phase(node_id, "closed")?;
    Ok(json!({ "closed": true }))
}

fn repoint_node(env: &GitEnv, node_id: &str, new_base: Option<&str>) -> Result<Value, String> {
    let stack = env.stack()?;
    let node = node_of(&stack, node_id)?.clone();
    let project = env.project()?;
    let repo_root = PathBuf::from(&project.main_repo_path);
    let remote = env.remote_name(&project, &repo_root);
    let default_branch = env.default_branch(&project);
    let (base_branch, base_ref) = match new_base.map(str::trim).filter(|base| !base.is_empty()) {
        Some(named) => {
            let branch = named
                .strip_prefix(&format!("{remote}/"))
                .or_else(|| named.strip_prefix("origin/"))
                .unwrap_or(named)
                .to_string();
            (branch.clone(), branch)
        }
        None => env.effective_base(&stack, node_id, &remote, &default_branch),
    };
    if node.branch.is_none() {
        return Ok(json!({ "node_id": node_id, "new_base": base_branch }));
    }
    env.realign(&node, &base_branch, &base_ref)?;
    Ok(json!({ "node_id": node_id, "new_base": base_branch }))
}

fn set_parents(env: &GitEnv, node_id: &str, parents: &[String]) -> Result<Value, String> {
    let stack = env.stack()?;
    let node = node_of(&stack, node_id)?.clone();
    // Validate before anything is written, as the plan writers do.
    let mut seen: Vec<&String> = Vec::new();
    for parent in parents {
        if parent == node_id {
            return Err(format!("node '{parent}' cannot be its own parent"));
        }
        if stack.node(parent).is_none() {
            return Err(format!("dangling parent ref: {parent}"));
        }
        if seen.contains(&parent) {
            return Err(format!("parent '{parent}' is named more than once"));
        }
        seen.push(parent);
    }
    let mut candidate = stack.clone();
    if let Some(candidate_node) = candidate.nodes.iter_mut().find(|n| n.node_id == node_id) {
        candidate_node.parents = parents.to_vec();
    }
    candidate
        .topo_order()
        .map_err(|e| format!("cycle detected: {e}"))?;
    // Refuse a dirty or running node before the plan moves.
    if let Some(branch) = node.branch.as_deref() {
        let project = env.project()?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        let _ = branch;
        let worktree = env.worktree_for(&node, &repo_root)?;
        env.refuse_dirty_or_running(&node, &worktree)?;
    }
    env.store
        .update(&env.project_id, &env.stack_id, |stack| {
            if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
                node.parents = parents.to_vec();
            }
        })
        .map_err(|e| e.to_string())?;
    if node.branch.is_some() {
        let updated = env.stack()?;
        let updated_node = node_of(&updated, node_id)?.clone();
        let project = env.project()?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        let remote = env.remote_name(&project, &repo_root);
        let default_branch = env.default_branch(&project);
        let (base_branch, base_ref) =
            env.effective_base(&updated, node_id, &remote, &default_branch);
        env.realign(&updated_node, &base_branch, &base_ref)?;
    }
    Ok(json!({ "node_id": node_id, "parents": parents }))
}

fn resolve_conflicts(env: &GitEnv, node_id: &str) -> Result<Value, String> {
    let stack = env.stack()?;
    let node = node_of(&stack, node_id)?.clone();
    let branch = node.branch.clone().ok_or_else(|| {
        format!("node '{node_id}' owns no branch yet, so it has no conflicts to find")
    })?;
    let project = env.project()?;
    let repo_root = PathBuf::from(&project.main_repo_path);
    let remote = env.remote_name(&project, &repo_root);
    let default_branch = env.default_branch(&project);
    let (_, base_ref) = env.effective_base(&stack, node_id, &remote, &default_branch);
    let worktree = env.worktree_for(&node, &repo_root)?;
    let conflicts = merge_tree_conflicts(&worktree, &branch, &base_ref)?;
    env.store
        .update(&env.project_id, &env.stack_id, |stack| {
            if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
                if conflicts.is_empty() {
                    if node
                        .internal_status
                        .as_ref()
                        .is_some_and(|status| status.kind == "has-conflicts")
                    {
                        node.internal_status = None;
                    }
                } else {
                    node.internal_status = Some(PrInternalStatus {
                        kind: "has-conflicts".to_string(),
                        note: None,
                        source: "override".to_string(),
                    });
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(json!({ "conflicts": conflicts }))
}

/// The paths `branch` and `base_ref` conflict on, probed with `git merge-tree --write-tree`:
/// no merge is started, so the worktree's HEAD, index and files are left exactly as they were.
fn merge_tree_conflicts(
    worktree: &Path,
    branch: &str,
    base_ref: &str,
) -> Result<Vec<String>, String> {
    let output = std::process::Command::new("git")
        .args([
            "merge-tree",
            "--write-tree",
            "--name-only",
            branch,
            base_ref,
        ])
        .current_dir(worktree)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git merge-tree in {}: {e}", worktree.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        return Ok(Vec::new());
    }
    if output.status.code() != Some(1) {
        return Err(format!(
            "git merge-tree {branch} {base_ref} in {}: {}",
            worktree.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut paths: Vec<String> = stdout
        .lines()
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .map(|line| line.trim().trim_matches('"').to_string())
        .filter(|line| !line.is_empty())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}
