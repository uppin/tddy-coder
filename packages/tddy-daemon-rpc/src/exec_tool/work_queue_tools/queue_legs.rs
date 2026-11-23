use std::sync::Arc;

use crate::work_queue::schedules::ScheduleCtx;

use tddy_tool_engine::work_queue_tools::WorkQueueOp;

use serde_json::Value;

use crate::exec_tool::work_queue_tools::{ops, GithubSource, OpEnv};

use super::DaemonStackTools;

impl DaemonStackTools {
    /// `work_queue_status`: refresh every node's derived status from the PRs the daemon's GitHub inbox
    /// holds — one inbox read per branch, never a request of its own — and answer the nodes. A
    /// refresh that could not be made is `refresh_error` in the answer, never hidden.
    pub(super) async fn status_through_inbox(
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
    pub(crate) async fn add_chain_health(
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
    pub(super) async fn graph_through_inbox(
        &self,
        env: OpEnv,
        github: GithubSource,
    ) -> Result<Value, String> {
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
    pub(super) async fn commit_flow_through_inbox(
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
    pub(crate) async fn merge_holdups(
        &self,
        project_id: &str,
        queue_id: &str,
    ) -> Result<std::collections::HashMap<String, (String, String)>, String> {
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
    pub(super) async fn schedules_through_inbox(
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
                        identity: self
                            .armer
                            .identity(self.vaults.clone(), self.user_resolver.clone()),
                        github_cli: self.github_cli.clone(),
                        clients: Arc::clone(&self.github),
                        terminals: self.terminals.clone(),
                        notification_bus: self.notification_bus.clone(),
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
                    armer: self.armer.clone(),
                },
            )
            .await;
        }
        answered
    }

    pub(super) async fn refresh_through_inbox(
        &self,
        env: &OpEnv,
        github: GithubSource,
    ) -> Result<(), String> {
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
