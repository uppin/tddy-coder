//! `PropagateWorkItem` and `LandWorkQueue` (`#workqueue` 14/14): the web's Propagate and Land wave
//! actions, run through the same engines the `work_item_propagate` and `work_queue_land` tools use.
//!
//! GitHub is reached as the project's account through the daemon's own REST client. A refused
//! propagation is an answer naming the state and the actions that fix it, not a failed call.

use tddy_core::changeset::WorkQueue;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    LandStepView, LandWorkQueueRequest, LandWorkQueueResponse, PropagateWorkItemRequest,
    PropagateWorkItemResponse,
};
use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_work_queue::engine::EngineRefusal;
use tddy_worktree_service::session_target::TargetRecord;

use super::links::LinkCall;
use super::schedules::ScheduleCtx;
use super::WorkQueueRpcHandler;
use crate::exec_tool::work_item_git_tools::git_work::{land_ops, GitEnv};
use crate::exec_tool::work_queue_tools::RealWorkQueueGithubClients;

/// One record per item that has a session, the worktree its metadata names and whether it is live.
fn records_of(
    sessions_base: &std::path::Path,
    queue: &WorkQueue,
    live: &tddy_daemon_kernel::live_sessions::LiveSessions,
) -> Vec<TargetRecord> {
    queue
        .nodes
        .iter()
        .filter_map(|item| {
            let session_id = item.session_id.clone()?;
            let dir = tddy_core::unified_session_dir_path(sessions_base, &session_id);
            let worktree = tddy_core::read_session_metadata(&dir)
                .ok()
                .and_then(|meta| meta.repo_path)
                .filter(|path| !path.is_empty())
                .map(std::path::PathBuf::from);
            Some(TargetRecord {
                is_active: live.contains(&session_id),
                project_id: String::new(),
                branch: item.branch.clone().unwrap_or_default(),
                base_ref: String::new(),
                codebase: None,
                worktree,
                session_id,
            })
        })
        .collect()
}

fn refusal_view(refusal: &EngineRefusal) -> PropagateWorkItemResponse {
    let mut view = PropagateWorkItemResponse {
        refusal_detail: refusal.to_string(),
        ..Default::default()
    };
    match refusal {
        EngineRefusal::LocalBranch { state, actions } => {
            view.refusal_kind = "local_branch".to_string();
            view.refusal_state = format!("{state:?}");
            view.refusal_actions = actions.iter().map(|a| a.to_string()).collect();
        }
        EngineRefusal::Preflight { .. } => view.refusal_kind = "preflight".to_string(),
        EngineRefusal::Failed(_) => view.refusal_kind = "failed".to_string(),
    }
    view
}

impl WorkQueueRpcHandler {
    async fn git_env_of(&self, call: &LinkCall, session_token: &str) -> Result<GitEnv, Status> {
        let sessions_base = self.sessions_base(&call.os_user)?;
        let store = self.stack_store(&sessions_base);
        let queue = store
            .read(&call.address.project_id, &call.address.queue_id)
            .map(|record| record.stack)
            .map_err(super::project_stacks::store_status)?;
        let live = self.terminals.live_sessions().await;
        Ok(GitEnv {
            projects_dir: store.projects_dir().to_path_buf(),
            records: records_of(&sessions_base, &queue, &live),
            store,
            project_id: call.address.project_id.clone(),
            queue_id: call.address.queue_id.clone(),
            os_user: call.os_user.clone(),
            identity: SessionAccountAccess::new(
                self.credential_vaults.clone(),
                self.user_resolver.clone(),
                session_token,
            ),
            github_cli: self.github_cli.clone(),
            clients: std::sync::Arc::new(RealWorkQueueGithubClients),
        })
    }

    pub(super) async fn propagate_work_item_rpc(
        &self,
        request: Request<PropagateWorkItemRequest>,
    ) -> Result<Response<PropagateWorkItemResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, PropagateWorkItemResponse>(
                "PropagateWorkItem",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let item_id = req.item_id.trim().to_string();
        if item_id.is_empty() {
            return Err(Status::invalid_argument("item_id is required"));
        }
        let env = self.git_env_of(&call, &req.session_token).await?;
        let force = req.force;
        let outcome = tokio::task::spawn_blocking(move || {
            land_ops::propagate_item(&env, &item_id, None, force)
        })
        .await
        .map_err(|e| Status::internal(format!("the propagation failed to finish: {e}")))?;
        Ok(Response::new(match outcome {
            Ok(done) => PropagateWorkItemResponse {
                propagated: true,
                onto: done.onto,
                commits_taken: done.commits_taken,
                ..Default::default()
            },
            Err(refusal) => refusal_view(&refusal),
        }))
    }

    pub(super) async fn land_work_queue_rpc(
        &self,
        request: Request<LandWorkQueueRequest>,
    ) -> Result<Response<LandWorkQueueResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, LandWorkQueueResponse>(
                "LandWorkQueue",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        if req.from_wave == 0 || req.to_wave < req.from_wave {
            return Err(Status::invalid_argument(
                "waves are numbered from 1 and from_wave must not exceed to_wave",
            ));
        }
        let (sessions_base, queue, trunk) = self.read_queue_and_trunk(&call)?;
        let store = self.stack_store(&sessions_base);
        let held_up = ScheduleCtx {
            store: &store,
            pull_requests: self.pull_requests.as_ref(),
            os_user: &call.os_user,
            sessions_base: &sessions_base,
            project_id: &call.address.project_id,
            queue_id: &call.address.queue_id,
            caller_session_id: &call.address.queue_id,
            session_token: &req.session_token,
            trunk: &trunk,
            armed_by: &call.os_user,
        }
        .merge_holdups(&queue)
        .await;
        let env = self.git_env_of(&call, &req.session_token).await?;
        let (from, to) = (req.from_wave, req.to_wave);
        let report =
            tokio::task::spawn_blocking(move || land_ops::land_range(&env, from, to, &held_up))
                .await
                .map_err(|e| Status::internal(format!("the land failed to finish: {e}")))?
                .map_err(Status::failed_precondition)?;
        let step = |item_id: &str, outcome: &str, detail: String| LandStepView {
            item_id: item_id.to_string(),
            outcome: outcome.to_string(),
            detail,
        };
        let mut steps: Vec<LandStepView> = report
            .landed
            .iter()
            .map(|(id, sha)| step(id, "landed", sha.clone()))
            .collect();
        steps.extend(
            report
                .propagated
                .iter()
                .map(|p| step(&p.item_id, "propagated", p.onto.clone())),
        );
        if let Some((id, why)) = report.stopped_at {
            steps.push(step(&id, "stopped", why));
        }
        Ok(Response::new(LandWorkQueueResponse { steps }))
    }
}
