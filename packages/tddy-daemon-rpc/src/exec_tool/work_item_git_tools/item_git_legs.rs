use tddy_tool_engine::start_ledger::StartState;

use std::time::Duration;

use tddy_tool_engine::start_session_args::StartSessionArgs;

use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_worktree_service::session_target::resolve_session_target;

use tddy_worktree_service::session_target::HomeCaller;

use std::sync::Arc;

use tddy_tool_engine::project_home_tools::ProjectHomePort;
use tddy_tool_engine::project_home_turn_tools::ProjectHomeTurnPort;

use serde_json::{json, Value};

use tddy_tool_engine::work_queue_tools::{WorkQueueCaller, WorkQueueToolPort};

use crate::exec_tool::work_item_git_tools::git_work;

use super::DaemonStackGitTools;

impl DaemonStackGitTools {
    /// Tell the queue's owner that `node_id`'s parents changed, when they did (`#workqueue` 8/14).
    /// `before` is the queue as it was before the change; a queue that cannot be read again says
    /// nothing, the change itself having succeeded.
    pub(crate) async fn announce_parents_changed(
        &self,
        caller: &WorkQueueCaller,
        queue_id: &str,
        node_id: &str,
        before: Option<tddy_core::work_queue_store::WorkQueueRecord>,
    ) {
        let Some(before) = before else { return };
        let Ok(after) = self.store.read(&caller.project_id, queue_id) else {
            return;
        };
        let parents_of = |record: &tddy_core::work_queue_store::WorkQueueRecord| {
            record
                .stack
                .node(node_id)
                .map(|node| node.parents.clone())
                .unwrap_or_default()
        };
        crate::work_queue::move_activity::publish_parents_changed(
            self.notification_bus.as_ref(),
            &crate::work_queue::move_activity::MovedQueue {
                queue_session_id: after.owner_session_id(),
                project_id: &caller.project_id,
                os_user: &self.os_user,
            },
            node_id,
            (parents_of(&before), parents_of(&after)),
            &after.stack,
            tddy_session_activity::work_queue_activity::Actor::Agent {
                session_id: caller.session_id.clone(),
            },
        )
        .await;
    }

    pub(crate) async fn run_blocking(
        &self,
        caller: &WorkQueueCaller,
        queue_id: &str,
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
            queue_id: queue_id.to_string(),
            records,
            os_user: self.os_user.clone(),
            identity: self.identity.clone(),
            github_cli: self.github_cli.clone(),
            clients: Arc::clone(&self.github),
            activity: crate::work_queue::move_activity::ActivitySink::on(
                self.notification_bus.as_ref(),
                tddy_session_activity::work_queue_activity::Actor::Agent {
                    session_id: caller.session_id.clone(),
                },
            ),
        };
        tokio::task::spawn_blocking(move || git_work::run_git_work(&env, work))
            .await
            .map_err(|e| format!("the stack git operation failed to finish: {e}"))?
    }

    pub(crate) async fn delegate_conflicts(
        &self,
        caller: &WorkQueueCaller,
        queue_id: &str,
        node_id: &str,
        paths: &[String],
    ) -> Result<(), String> {
        let record = self
            .store
            .read(&caller.project_id, queue_id)
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

    pub(crate) async fn spawn_child(
        &self,
        caller: &WorkQueueCaller,
        queue_id: &str,
        node_id: &str,
        session_type: Option<String>,
        model: Option<String>,
    ) -> Result<Value, String> {
        let record = self
            .store
            .read(&caller.project_id, queue_id)
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
                "work_item_spawn_session: no session_type named and the caller records none"
                    .to_string()
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
            work_queue_parent: caller.session_id.clone(),
            work_queue_parent_daemon_instance_id: self.stacks.daemon_instance_id(),
            work_item_id: node_id.to_string(),
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
            .update(&caller.project_id, queue_id, |stack| {
                if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
                    node.session_id = Some(child_for_store.clone());
                    node.branch = Some(branch.clone());
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(json!({ "session_id": child }))
    }
}
