//! The PR-stack plan and read tools ([`tddy_tool_engine::work_queue_tools`]) answered on this daemon
//! (`#orchestrators` 7/11).
//!
//! A stack tool is routed here from `run_locally`, ahead of the local runner, as the project-home
//! tools are. The caller is classified from its own records ([`work_queue_caller_of`]), by role alone:
//! an orchestrator-role session (its changeset's `queue_id` being its own, default stack), or a
//! session bound to a node of a stack. No recipe is read (developer, 2026-11-14). A
//! session with none of those roles, or calling a tool its role is not offered, is refused as
//! permission denied. A call whose `daemon_instance_id` names another host is forwarded whole to
//! that host, which serves it for its own stacks once the caller's owner confirms the caller, one
//! hop as the project-home tools are (`project_home_remote`).
//!
//! The work is [`DaemonStackTools`]: the host's [`WorkQueueStore`] (`<sessions base>/projects`),
//! `tddy_work_queue`'s stack operations and PR reads over it, and GitHub reached as the **project's
//! account** — the token the project row's assignment (or its Local gh) resolves, never the calling
//! session's, which a read-only session does not have.
//!
//! Feature doc: docs/ft/coder/pr-stacking.md § PR-management tools.
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-plan-tools.md.

mod github;
mod ops;
mod shape;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tddy_core::work_queue_store::{WorkQueueRecord, WorkQueueStore};
use tddy_github::pr_api::{GithubPrApi, GithubPrInsightApi, RealGithubPrApi};
use tddy_host_service::GithubCli;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_tool_engine::work_queue_access::WorkQueueAccess;
use tddy_tool_engine::work_queue_tools::{
    execute_stack_tool, is_stack_tool_offered_to, WorkQueueCaller, WorkQueueOp, WorkQueueScope,
    WorkQueueToolPort,
};
use tddy_tool_engine::ToolOutcome;
use tddy_worktree_service::pull_request_view::SessionPullRequests;

use super::project_home::caller::record_call;
use super::ExecToolRpcHandler;
use github::GithubSource;
use ops::OpEnv;

/// A GitHub client a stack tool calls through: the lifecycle operations and the reads, for one
/// repository, authenticated as one account.
pub trait WorkQueueGithub: GithubPrApi + GithubPrInsightApi {}

impl<T: GithubPrApi + GithubPrInsightApi + ?Sized> WorkQueueGithub for T {}

/// How the stack tools reach GitHub: a client for a project's repository, authenticated as the
/// project account's token. The daemon's own is [`RealWorkQueueGithubClients`]; a suite attaches
/// another with [`RpcHandlers::attach_stack_github`](crate::RpcHandlers::attach_stack_github).
pub trait WorkQueueGithubClients: Send + Sync {
    /// A client for `repo` (`owner/repo`) sending `token` — the project account's.
    fn client_for(&self, repo: &str, token: &str) -> Box<dyn WorkQueueGithub>;
}

/// The GitHub REST API itself.
pub struct RealWorkQueueGithubClients;

impl WorkQueueGithubClients for RealWorkQueueGithubClients {
    fn client_for(&self, repo: &str, token: &str) -> Box<dyn WorkQueueGithub> {
        Box::new(RealGithubPrApi::with_token(repo, token))
    }
}

/// `session_id` as a caller on this host, with the access its records give it (it owns a queue, or an
/// item of one links to it); `None` for a session offered no work tool. A session whose records
/// cannot be read is refused: its access cannot be known.
pub(super) fn work_queue_caller_of(
    sessions_base: &Path,
    session_id: &str,
) -> Result<Option<(WorkQueueCaller, WorkQueueAccess)>, Status> {
    tddy_tool_engine::work_queue_tools::caller_records::work_queue_caller_from_records(
        sessions_base,
        session_id,
    )
    .map_err(|e| Status::permission_denied(format!("the stack tools are refused: {e}")))
}

impl ExecToolRpcHandler {
    /// Serve `req`'s stack tool: classify the caller, refuse a caller its role does not entitle,
    /// forward a call naming another host, else run it on this host's stacks and record the call.
    pub(super) async fn run_stack_tool(
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
        let (caller, access) = work_queue_caller_of(sessions_base, &req.session_id)?.ok_or_else(|| {
            Status::permission_denied(format!(
                "tool {tool} is served only to a session that owns a work queue, or one an item of a \
                 work queue links to"
            ))
        })?;
        if !is_stack_tool_offered_to(access, tool) {
            return Err(Status::permission_denied(format!(
                "tool {tool} is not offered to a session that is only addressed by a work queue: it \
                 may read a queue's status and its pull requests only"
            )));
        }
        let response = match self.served_on_target_host(req).await? {
            Some(answered) => answered,
            None => {
                let os_user = self.os_user_of(&req.session_token)?;
                let port = self.work_queue_tool_port(req, os_user, sessions_base);
                response_of(execute_stack_tool(&port, &caller, tool, &req.args_json).await)
            }
        };
        record_call(sessions_base, req, &response);
        Ok(response)
    }

    /// Forward `req` whole to the host its `daemon_instance_id` names when it names a queue there
    /// (`names_a_queue_on_another_host`), recording the call: that host decides the caller's access
    /// from the queue it holds, which this host has no record of (`#workqueue` 5/14). `None` for any
    /// other call, which this host checks itself.
    pub(super) async fn forwarded_to_the_queues_host(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<Option<ExecuteToolResponse>, Status> {
        let here = tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(&self.config);
        if !tddy_tool_engine::work_queue_access::names_a_queue_on_another_host(
            &req.args_json,
            &here,
        ) {
            return Ok(None);
        }
        let Some(answered) = self.served_on_target_host(req).await? else {
            return Ok(None);
        };
        record_call(sessions_base, req, &answered);
        Ok(Some(answered))
    }

    /// Serve a stack tool called by a session another host holds — forwarded here by that host,
    /// which holds the stack's owner's side of the call — once the daemon that owns the session has
    /// confirmed it holds an orchestrator role. Such a caller owns no stack here, so a call must name
    /// the stack it acts on.
    pub(super) async fn serve_stack_tool_for_caller_held_elsewhere(
        &self,
        req: &ExecuteToolRequest,
        caller: WorkQueueCaller,
        os_user: String,
        sessions_base: &Path,
    ) -> ExecuteToolResponse {
        let port = self.work_queue_tool_port(req, os_user, sessions_base);
        response_of(execute_stack_tool(&port, &caller, &req.tool_name, &req.args_json).await)
    }

    /// [`DaemonStackTools`] over this host for `req`'s caller, whose sessions are `os_user`'s in
    /// `sessions_base`.
    pub(super) fn work_queue_tool_port(
        &self,
        req: &ExecuteToolRequest,
        os_user: String,
        sessions_base: &Path,
    ) -> DaemonStackTools {
        DaemonStackTools {
            store: WorkQueueStore::new(sessions_base),
            sessions_base: sessions_base.to_path_buf(),
            os_user,
            daemon_instance_id: tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(
                &self.config,
            ),
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
                .unwrap_or_else(|| Arc::new(RealWorkQueueGithubClients)),
            pull_requests: Arc::clone(&self.project_home_host.pull_requests),
            caller_session_id: req.session_id.clone(),
            session_token: req.session_token.clone(),
            terminals: self.project_home_host.terminals.clone(),
        }
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

/// [`WorkQueueToolPort`] over this host's stacks, for one caller.
pub(super) struct DaemonStackTools {
    /// This host's stacks, `<sessions base>/projects/<project_id>/stacks/<queue_id>.yaml`.
    store: WorkQueueStore,
    /// The caller's sessions, where a node's child session is read.
    sessions_base: PathBuf,
    os_user: String,
    /// This host, as the eligible-daemon roster names it.
    daemon_instance_id: String,
    /// The signed-in person's vault, which the project's assigned GitHub account is read from.
    identity: SessionAccountAccess,
    /// The `gh` a Local gh project's token is asked of.
    github_cli: GithubCli,
    github: Arc<dyn WorkQueueGithubClients>,
    /// The inbox and project credentials a queue's status is read through (`#workqueue` 6/14).
    pull_requests: Arc<dyn SessionPullRequests>,
    caller_session_id: String,
    session_token: String,
    /// The live terminals, which say whether an item's session is running.
    terminals: tddy_session_lifecycle::terminal_session_adapter::DaemonTerminalSessionStore,
}

impl DaemonStackTools {
    /// `work_queue_status`: refresh every node's derived status from the PRs the daemon's GitHub inbox
    /// holds — one inbox read per branch, never a request of its own — and answer the nodes. A
    /// refresh that could not be made is `refresh_error` in the answer, never hidden.
    async fn status_through_inbox(
        &self,
        env: OpEnv,
        github: GithubSource,
    ) -> Result<Value, String> {
        let project_id = env.project_id.clone();
        let queue_id = env.queue_id.clone();
        let sessions_base = env.sessions_base.clone();
        // The trunk the line edges end at, asked before the refresh takes the source.
        let (trunk_ref, github) = tokio::task::spawn_blocking(move || {
            let trunk_ref = github.default_branch();
            (trunk_ref, github)
        })
        .await
        .map_err(|e| format!("the stack operation failed to finish: {e}"))?;
        let refresh = self.refresh_through_inbox(&env, github).await;
        let refresh_error = refresh.err();
        // The store's locks are blocking.
        let (answer, stack) = tokio::task::spawn_blocking(move || {
            let answer = ops::status_answer(&env, refresh_error);
            let stack = env.stack();
            (answer, stack)
        })
        .await
        .map_err(|e| format!("the stack operation failed to finish: {e}"))?;
        let mut answer = answer?;
        if let (Ok(stack), Ok(trunk_ref)) = (stack, trunk_ref) {
            self.add_chain_health(
                &mut answer,
                &stack,
                tddy_work_queue::lineage::trunk_branch_of(&trunk_ref),
                &project_id,
                queue_id.as_deref().unwrap_or(&self.caller_session_id),
                &sessions_base,
            )
            .await;
        }
        Ok(answer)
    }

    /// Each node's `chain_health` (`#workqueue` 12/14: its `leaks` and `line`), read through the inbox
    /// and the owner's checkout. Optional input: a queue whose GitHub cannot be read answers its nodes
    /// without it, the way the status never fails over one unreadable source.
    async fn add_chain_health(
        &self,
        answer: &mut Value,
        queue: &tddy_core::changeset::WorkQueue,
        trunk: &str,
        project_id: &str,
        queue_id: &str,
        sessions_base: &std::path::Path,
    ) {
        use crate::work_queue::chain_health as health;
        let Ok(site) = self.pull_requests.site_for_session(
            &self.os_user,
            &self.caller_session_id,
            project_id,
            &self.session_token,
        ) else {
            return;
        };
        let line = tddy_work_queue::lineage::line_edges_through_inbox(
            &site.inbox,
            &site.repo,
            queue,
            trunk,
        )
        .await;
        let repo_root = tddy_core::repo_root_for_session(
            &tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, queue_id),
        );
        let found = tddy_work_queue::chain_health_reads::queue_chain_health(
            &site.inbox,
            &site.repo,
            queue,
            &line,
            repo_root,
        )
        .await;
        if let Some(nodes) = answer.get_mut("nodes").and_then(Value::as_array_mut) {
            for node in nodes {
                let id = node.get("node_id").and_then(Value::as_str).unwrap_or("");
                if let Some(h) = found.iter().find(|h| h.item_id == id) {
                    node["chain_health"] = health::health_json(h);
                }
            }
        }
    }

    /// `work_queue_graph`: each item's line edge read through the inbox (else its planned base) and
    /// the stored dependency edges. A GitHub that cannot be read is `unavailable_reason`, with the
    /// planned bases standing in.
    async fn graph_through_inbox(&self, env: OpEnv, github: GithubSource) -> Result<Value, String> {
        let project_id = env.project_id.clone();
        let (queue, trunk_ref) = tokio::task::spawn_blocking(move || {
            Ok::<_, String>((env.stack()?, github.default_branch()?))
        })
        .await
        .map_err(|e| format!("the graph read failed to finish: {e}"))??;
        let trunk = tddy_work_queue::lineage::trunk_branch_of(&trunk_ref);
        let site = self.pull_requests.site_for_session(
            &self.os_user,
            &self.caller_session_id,
            &project_id,
            &self.session_token,
        );
        Ok(match site {
            Ok(site) => {
                let graph =
                    tddy_work_queue::lineage::queue_graph(&site.inbox, &site.repo, &queue, trunk)
                        .await;
                ops::graph_json(&graph, None)
            }
            Err(status) => {
                let graph = tddy_work_queue::lineage::QueueGraph {
                    line: tddy_work_queue::lineage::planned_line_edges(&queue, trunk),
                    dependencies: tddy_work_queue::lineage::dependencies_of(&queue),
                };
                ops::graph_json(&graph, Some(status.message()))
            }
        })
    }

    /// `work_item_inbox` / `work_item_outbox`: the commit flow of one item, over line edges read through
    /// the inbox (else the planned bases, named in `unavailable_reason`), in the owner's checkout.
    async fn commit_flow_through_inbox(
        &self,
        env: OpEnv,
        github: GithubSource,
        op: WorkQueueOp,
    ) -> Result<Value, String> {
        use crate::work_queue::commit_flow as flow;
        let project_id = env.project_id.clone();
        let queue_id = env.queue_id.clone();
        let sessions_base = env.sessions_base.clone();
        let (queue, trunk_ref) = tokio::task::spawn_blocking(move || {
            Ok::<_, String>((env.stack()?, github.default_branch()?))
        })
        .await
        .map_err(|e| format!("the commit flow read failed to finish: {e}"))??;
        let trunk = tddy_work_queue::lineage::trunk_branch_of(&trunk_ref);
        let site = flow::flow_site(flow::FlowInputs {
            pull_requests: self.pull_requests.as_ref(),
            os_user: &self.os_user,
            sessions_base: &sessions_base,
            queue_id: queue_id.as_deref().unwrap_or(&self.caller_session_id),
            caller_session_id: &self.caller_session_id,
            project_id: &project_id,
            session_token: &self.session_token,
            queue: &queue,
            trunk,
        })
        .await?;
        let reason = site.unavailable_reason.clone();
        match op {
            WorkQueueOp::Inbox { item_id } => {
                flow::check_item(&queue, &site, &item_id)?;
                let entries =
                    flow::read_flow(&site, move |f, items| f.inbox(items, &item_id)).await?;
                Ok(flow::json::inbox(entries, &reason))
            }
            WorkQueueOp::Outbox { item_id } => {
                flow::check_item(&queue, &site, &item_id)?;
                let entries =
                    flow::read_flow(&site, move |f, items| f.outbox(items, &item_id)).await?;
                Ok(flow::json::outbox(entries, &reason))
            }
            _ => Err("not a commit flow operation".to_string()),
        }
    }

    /// What holds each item's merge up, judged as an armed merge would be (`#workqueue` 14/14):
    /// `item -> (condition, detail)` for every item with a branch whose merge conditions are not all
    /// met. Nothing is armed or stored — the schedules are evaluated and dropped.
    pub(super) async fn merge_holdups(
        &self,
        project_id: &str,
        queue_id: &str,
    ) -> Result<std::collections::HashMap<String, (String, String)>, String> {
        use crate::work_queue::schedules::ScheduleCtx;
        let github = GithubSource {
            projects_dir: self.store.projects_dir().to_path_buf(),
            project_id: project_id.to_string(),
            os_user: self.os_user.clone(),
            identity: self.identity.clone(),
            github_cli: self.github_cli.clone(),
            clients: Arc::clone(&self.github),
        };
        let trunk_ref = tokio::task::spawn_blocking(move || github.default_branch())
            .await
            .map_err(|e| format!("the merge conditions failed to be read: {e}"))??;
        let trunk = tddy_work_queue::lineage::trunk_branch_of(&trunk_ref);
        let queue = self
            .store
            .read(project_id, queue_id)
            .map(|record| record.stack)
            .map_err(|e| e.to_string())?;
        let ctx = ScheduleCtx {
            store: &self.store,
            pull_requests: self.pull_requests.as_ref(),
            os_user: &self.os_user,
            sessions_base: &self.sessions_base,
            project_id,
            queue_id,
            caller_session_id: &self.caller_session_id,
            session_token: &self.session_token,
            trunk,
            armed_by: &self.os_user,
        };
        Ok(ctx.merge_holdups(&queue).await)
    }

    /// `work_item_schedule`, `work_queue_schedule_wave`, `work_item_unschedule` and
    /// `work_queue_schedules`: armed merges and propagations, their conditions read through the inbox.
    async fn schedules_through_inbox(
        &self,
        env: OpEnv,
        github: GithubSource,
        op: WorkQueueOp,
    ) -> Result<Value, String> {
        use crate::work_queue::schedules::{json, ScheduleCtx};
        let trunk_ref = tokio::task::spawn_blocking(move || github.default_branch())
            .await
            .map_err(|e| format!("the schedule operation failed to finish: {e}"))??;
        let trunk = tddy_work_queue::lineage::trunk_branch_of(&trunk_ref);
        let queue_id = env.queue_id.clone();
        let ctx = ScheduleCtx {
            store: &env.store,
            pull_requests: self.pull_requests.as_ref(),
            os_user: &self.os_user,
            sessions_base: &env.sessions_base,
            project_id: &env.project_id,
            queue_id: queue_id.as_deref().unwrap_or(&self.caller_session_id),
            caller_session_id: &self.caller_session_id,
            session_token: &self.session_token,
            trunk,
            armed_by: &self.os_user,
        };
        let changes_schedules = matches!(
            op,
            WorkQueueOp::Schedule { .. }
                | WorkQueueOp::ScheduleWave { .. }
                | WorkQueueOp::Unschedule { .. }
        );
        let answered = match op {
            WorkQueueOp::Schedule {
                item_id,
                kind,
                skip_review,
            } => ctx
                .arm_item(&item_id, &kind, skip_review)
                .await
                .map(json::schedule),
            WorkQueueOp::ScheduleWave { wave, skip_review } => ctx
                .arm_wave(wave, skip_review)
                .await
                .map(|(views, reason)| json::schedules(views, &reason)),
            WorkQueueOp::Unschedule { item_id } => ctx
                .disarm(&item_id)
                .await
                .map(|(views, reason)| json::schedules(views, &reason)),
            WorkQueueOp::Schedules => ctx
                .list()
                .await
                .map(|(views, reason)| json::schedules(views, &reason)),
            _ => return Err("not a schedule operation".to_string()),
        }
        .map_err(|e| e.to_string());
        if changes_schedules && answered.is_ok() {
            // `#workqueue` 14/14: what is already met fires now, and the queue is watched from here on.
            crate::work_queue::schedule_watch::after_arming(
                crate::work_queue::schedule_watch::WatchInputs {
                    source: crate::work_queue::land::GitEnvSource {
                        identity: self.identity.clone(),
                        github_cli: self.github_cli.clone(),
                        clients: Arc::clone(&self.github),
                        terminals: self.terminals.clone(),
                    },
                    pull_requests: Arc::clone(&self.pull_requests),
                    store: self.store.clone(),
                    sessions_base: self.sessions_base.clone(),
                    os_user: self.os_user.clone(),
                    address: tddy_service::proto::work_queue::WorkQueueAddress {
                        project_id: env.project_id.clone(),
                        queue_id: queue_id
                            .clone()
                            .unwrap_or_else(|| self.caller_session_id.clone()),
                        daemon_instance_id: self.daemon_instance_id.clone(),
                    },
                    session_token: self.session_token.clone(),
                },
            )
            .await;
        }
        answered
    }

    async fn refresh_through_inbox(&self, env: &OpEnv, github: GithubSource) -> Result<(), String> {
        let read_env = env.clone();
        let inputs = tokio::task::spawn_blocking(move || ops::refresh_inputs(&read_env, &github))
            .await
            .map_err(|e| format!("the status refresh failed to finish: {e}"))??;
        let site = self
            .pull_requests
            .site_for_session(
                &self.os_user,
                &self.caller_session_id,
                &env.project_id,
                &self.session_token,
            )
            .map_err(|status| status.message().to_string())?;
        let views = tddy_work_queue::inbox_reads::assemble_views_through_inbox(
            &inputs.planner_dir,
            &env.sessions_base,
            &inputs.stack,
            &site.inbox,
            &site.repo,
        )
        .await
        .map_err(|e| format!("assemble views: {e}"))?;
        let persist_env = env.clone();
        let default_branch = inputs.default_branch;
        tokio::task::spawn_blocking(move || {
            ops::persist_derived_statuses(&persist_env, &views, &default_branch)
        })
        .await
        .map_err(|e| format!("the status refresh failed to finish: {e}"))?
    }
}

#[async_trait]
impl WorkQueueToolPort for DaemonStackTools {
    fn daemon_instance_id(&self) -> String {
        self.daemon_instance_id.clone()
    }

    async fn list_stacks(&self, project_id: &str) -> Result<Vec<WorkQueueRecord>, String> {
        self.store.list(project_id).map_err(|e| e.to_string())
    }

    async fn access_to(
        &self,
        caller: &WorkQueueCaller,
        queue_id: &str,
    ) -> Result<WorkQueueAccess, String> {
        tddy_tool_engine::work_queue_tools::caller_records::access_to_queue_here(
            &self.sessions_base,
            caller,
            queue_id,
        )
    }

    async fn run_stack_op(&self, scope: &WorkQueueScope, op: WorkQueueOp) -> Result<Value, String> {
        let env = OpEnv {
            store: self.store.clone(),
            project_id: scope.project_id.clone(),
            queue_id: scope.queue_id.clone(),
            sessions_base: self.sessions_base.clone(),
        };
        let github = GithubSource {
            projects_dir: self.store.projects_dir().to_path_buf(),
            project_id: scope.project_id.clone(),
            os_user: self.os_user.clone(),
            identity: self.identity.clone(),
            github_cli: self.github_cli.clone(),
            clients: Arc::clone(&self.github),
        };
        if matches!(op, WorkQueueOp::Status) {
            return self.status_through_inbox(env, github).await;
        }
        if matches!(op, WorkQueueOp::Graph) {
            return self.graph_through_inbox(env, github).await;
        }
        if let WorkQueueOp::Inbox { .. } | WorkQueueOp::Outbox { .. } = &op {
            return self.commit_flow_through_inbox(env, github, op).await;
        }
        if let WorkQueueOp::Schedule { .. }
        | WorkQueueOp::ScheduleWave { .. }
        | WorkQueueOp::Unschedule { .. }
        | WorkQueueOp::Schedules = &op
        {
            return self.schedules_through_inbox(env, github, op).await;
        }
        // The store's locks, git and GitHub are all blocking.
        tokio::task::spawn_blocking(move || ops::run(&env, &github, op))
            .await
            .map_err(|e| format!("the stack operation failed to finish: {e}"))?
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_changeset::work_queue_fixtures::{a_stack_host, A_PROJECT};
    use tddy_core::changeset::{Changeset, WorkItem, WorkQueue};

    use super::*;

    fn a_queue_linking(child: Option<&str>) -> WorkQueue {
        WorkQueue {
            version: 1,
            nodes: vec![WorkItem {
                node_id: "n1".to_string(),
                title: "Store".to_string(),
                session_id: child.map(str::to_string),
                ..WorkItem::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_session_owning_a_queue_is_its_owner_whatever_its_role() {
        // Given a session with no orchestrator role that owns a queue
        let host = a_stack_host();
        host.orchestrator("owner", a_queue_linking(None));

        // When it is read
        let caller = work_queue_caller_of(host.base(), "owner").unwrap();

        // Then it is the owner of its project's queue
        assert_eq!(
            caller,
            Some((
                WorkQueueCaller {
                    session_id: "owner".to_string(),
                    project_id: A_PROJECT.to_string(),
                },
                WorkQueueAccess::Owner
            ))
        );
    }

    #[test]
    fn an_orchestrator_role_session_owning_no_queue_has_no_access() {
        // Given a project-home session that owns no queue (access never reads the role)
        let host = a_stack_host();
        let dir = host.session("main", Changeset::default());
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.session_role = Some("project-home".to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();

        // When it is read
        let caller = work_queue_caller_of(host.base(), "main").unwrap();

        // Then it has none
        assert_eq!(caller, None);
    }

    #[test]
    fn a_session_with_no_record_is_refused_rather_than_read_as_ordinary() {
        // Given a session id with nothing on disk
        let host = a_stack_host();

        // When it is read
        let refusal =
            work_queue_caller_of(host.base(), "ghost").expect_err("its access cannot be known");

        // Then it is refused as permission denied, naming the session
        assert_eq!(refusal.code(), tddy_rpc::Code::PermissionDenied);
        assert!(refusal.message().contains("ghost"), "{}", refusal.message());
    }
}
