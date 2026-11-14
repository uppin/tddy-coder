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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tddy_core::project_stack_store::ProjectStackStore;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_host_service::GithubCli;
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
use tddy_worktree_service::session_target::{resolve_session_target, HomeCaller};

use super::project_home::caller::record_call;
use super::project_home::DaemonProjectHome;
use super::stack_tools::{stack_caller_of, DaemonStackTools, StackGithubClients};
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
        work: git_work::GitWork,
    ) -> Result<Value, String> {
        let records = self
            .home
            .target_records()
            .await
            .map_err(|e| format!("{e:#}"))?;
        let env = git_work::GitEnv {
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
        tokio::task::spawn_blocking(move || git_work::run_git_work(&env, work))
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
                        git_work::GitWork::ResolveConflicts {
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
                self.run_blocking(caller, stack_id, git_work::GitWork::Merge { node_id })
                    .await
            }
            StackGitOp::Close { node_id } => {
                self.run_blocking(caller, stack_id, git_work::GitWork::Close { node_id })
                    .await
            }
            StackGitOp::Repoint { node_id, new_base } => {
                self.run_blocking(
                    caller,
                    stack_id,
                    git_work::GitWork::Repoint { node_id, new_base },
                )
                .await
            }
            StackGitOp::SetParents { node_id, parents } => {
                self.run_blocking(
                    caller,
                    stack_id,
                    git_work::GitWork::SetParents { node_id, parents },
                )
                .await
            }
        }
    }
}

mod git_work;
