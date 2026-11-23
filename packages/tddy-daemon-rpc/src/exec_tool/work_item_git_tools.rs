//! The PR-stack git tools ([`tddy_tool_engine::work_item_git_tools`]) answered on this daemon
//! (`#orchestrators` 10/11).
//!
//! Routed from `run_locally` beside the plan tools ([`super::work_queue_tools`]) and classified the same
//! way (`work_queue_caller_of`): only the owner of the addressed queue is served, and a call naming another
//! host with `daemon_instance_id` is forwarded whole to that host, one hop.
//!
//! Every git step runs **here, in the node's own worktree** — the worktree of the project's
//! repository that holds the node's branch — never in the caller's checkout:
//!
//! - a rebase (`work_item_repoint`, `work_item_set_parents`) is refused, untouched, while that worktree has
//!   uncommitted changes or the node's session is running, then force-pushes with lease to the
//!   project's remote with the project account's token
//!   ([`tddy_worktree_service::session_git_ops::PushIdentity`]);
//! - `work_item_resolve_conflicts` finds the conflicting paths with `git merge-tree --write-tree`,
//!   leaving the worktree exactly as it was, and with `delegate` hands them to the node's session
//!   through `SendSessionMessage` (running) or `ResumeSession` with the message (stopped) —
//!   `#orchestrators` 4/11's turn port;
//! - `work_item_spawn_session` starts the node's session through the daemon's start path, its stack parent
//!   the caller, and binds the node to the started session;
//! - `work_item_merge` / `work_item_close` reach GitHub as the project's account.
//!
//! A merge keeps its crash-safety journal beside the queue, in its owning session's directory
//! (`<sessions>/<queue_id>/work-queue.op.json`).
//!
//! Feature doc: docs/ft/coder/pr-stacking.md § Merge and repoint.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tddy_core::work_queue_store::WorkQueueStore;
use tddy_host_service::GithubCli;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_tool_engine::work_item_git_tools::{
    execute_stack_git_tool, WorkItemGitOp, WorkItemGitToolPort,
};
use tddy_tool_engine::work_queue_tools::{WorkQueueCaller, WorkQueueToolPort};
use tddy_tool_engine::ToolOutcome;

use super::project_home::caller::record_call;
use super::project_home::DaemonProjectHome;
use super::work_queue_tools::{work_queue_caller_of, DaemonStackTools, WorkQueueGithubClients};
use super::ExecToolRpcHandler;

impl ExecToolRpcHandler {
    /// Serve `req`'s stack git tool: read the caller's records (the owner of its own queue and
    /// nothing else is served — checked against the addressed queue by the tool), forward a call
    /// naming another host, else run it on this host's queue and record the call.
    pub(super) async fn run_stack_git_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        if let Some(answered) = self
            .forwarded_to_the_queues_host(req, sessions_base)
            .await?
        {
            return Ok(answered);
        }
        let tool = &req.tool_name;
        let (caller, access) =
            work_queue_caller_of(sessions_base, &req.session_id)?.ok_or_else(|| {
                Status::permission_denied(format!(
                    "tool {tool} is served only to a session that owns a work queue"
                ))
            })?;
        if !tddy_tool_engine::work_item_git_tools::is_stack_git_tool_offered_to(access, tool) {
            return Err(Status::permission_denied(format!(
                "tool {tool} is not offered to a session that only a work queue addresses: the git \
                 tools are served to a queue's owner alone"
            )));
        }
        let response = match self.served_on_target_host(req).await? {
            Some(answered) => answered,
            None => {
                let os_user = self.os_user_of(&req.session_token)?;
                let port = self.work_item_git_tool_port(req, os_user, sessions_base)?;
                response_of(execute_stack_git_tool(&port, &caller, tool, &req.args_json).await)
            }
        };
        record_call(sessions_base, req, &response);
        Ok(response)
    }

    /// Serve a stack git tool called by a session another host holds — forwarded here by that
    /// host — once the daemon that owns the session has confirmed the session exists.
    /// Such a caller owns no stack here, so a call must name the stack it acts on.
    pub(super) async fn serve_stack_git_tool_for_caller_held_elsewhere(
        &self,
        req: &ExecuteToolRequest,
        caller: WorkQueueCaller,
        os_user: String,
        sessions_base: &Path,
    ) -> ExecuteToolResponse {
        match self.work_item_git_tool_port(req, os_user, sessions_base) {
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
    fn work_item_git_tool_port(
        &self,
        req: &ExecuteToolRequest,
        os_user: String,
        sessions_base: &Path,
    ) -> Result<DaemonStackGitTools, Status> {
        let stacks = self.work_queue_tool_port(req, os_user.clone(), sessions_base);
        let local_id =
            tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(&self.config);
        let home = self.project_home_port(req, os_user.clone(), sessions_base, None, local_id)?;
        let store = WorkQueueStore::new(sessions_base);
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
                .work_queue_github
                .get()
                .cloned()
                .unwrap_or_else(|| Arc::new(super::work_queue_tools::RealWorkQueueGithubClients)),
            notification_bus: self.project_home_host.notification_bus.clone(),
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

mod item_git_legs;

/// [`WorkItemGitToolPort`] over this host's stacks and the nodes' worktrees, for one caller: the plan
/// tools' port for the store and GitHub, and the project-home port for the node's session (its
/// target, its turn, its resume, a start).
pub(super) struct DaemonStackGitTools {
    pub(super) stacks: DaemonStackTools,
    pub(super) home: DaemonProjectHome,
    store: WorkQueueStore,
    sessions_base: PathBuf,
    projects_dir: PathBuf,
    os_user: String,
    identity: SessionAccountAccess,
    github_cli: GithubCli,
    github: Arc<dyn WorkQueueGithubClients>,
    /// Where a change of a work item is published for its queue's owner (`#workqueue` 8/14, 14/14).
    notification_bus:
        Option<Arc<tddy_session_activity::session_notifications::SessionNotificationBus>>,
}

#[async_trait]
impl WorkItemGitToolPort for DaemonStackGitTools {
    async fn access_to(
        &self,
        caller: &WorkQueueCaller,
        queue_id: &str,
    ) -> Result<tddy_tool_engine::work_queue_access::WorkQueueAccess, String> {
        self.stacks.access_to(caller, queue_id).await
    }

    async fn run_stack_git_op(
        &self,
        caller: &WorkQueueCaller,
        queue_id: &str,
        op: WorkItemGitOp,
    ) -> Result<Value, String> {
        match op {
            WorkItemGitOp::SpawnChild {
                node_id,
                session_type,
                model,
            } => {
                self.spawn_child(caller, queue_id, &node_id, session_type, model)
                    .await
            }
            WorkItemGitOp::ResolveConflicts { node_id, delegate } => {
                let answer = self
                    .run_blocking(
                        caller,
                        queue_id,
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
                        self.delegate_conflicts(caller, queue_id, &node_id, &paths)
                            .await?;
                    }
                }
                Ok(answer)
            }
            WorkItemGitOp::Merge { node_id } => {
                self.run_blocking(caller, queue_id, git_work::GitWork::Merge { node_id })
                    .await
            }
            WorkItemGitOp::Close { node_id } => {
                self.run_blocking(caller, queue_id, git_work::GitWork::Close { node_id })
                    .await
            }
            WorkItemGitOp::Repoint { node_id, new_base } => {
                self.run_blocking(
                    caller,
                    queue_id,
                    git_work::GitWork::Repoint { node_id, new_base },
                )
                .await
            }
            WorkItemGitOp::Propagate { item_id, force } => {
                self.run_blocking(
                    caller,
                    queue_id,
                    git_work::GitWork::Propagate { item_id, force },
                )
                .await
            }
            WorkItemGitOp::Land { from_wave, to_wave } => {
                let held_up = self
                    .stacks
                    .merge_holdups(&caller.project_id, queue_id)
                    .await?;
                self.run_blocking(
                    caller,
                    queue_id,
                    git_work::GitWork::Land {
                        from_wave,
                        to_wave,
                        held_up,
                    },
                )
                .await
            }
            WorkItemGitOp::SetParents { node_id, parents } => {
                let before = self.store.read(&caller.project_id, queue_id).ok();
                let answer = self
                    .run_blocking(
                        caller,
                        queue_id,
                        git_work::GitWork::SetParents {
                            node_id: node_id.clone(),
                            parents,
                        },
                    )
                    .await?;
                self.announce_parents_changed(caller, queue_id, &node_id, before)
                    .await;
                Ok(answer)
            }
        }
    }
}

pub(crate) mod git_work;
