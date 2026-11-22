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

/// What builds a [`GitEnv`] for a queue of this host: the caller's identity, the `gh`, GitHub, and the
/// live terminals. Cheap to clone, so a task that outlives the call carries its own.
#[derive(Clone)]
pub(crate) struct GitEnvSource {
    pub(crate) identity: SessionAccountAccess,
    pub(crate) github_cli: tddy_host_service::GithubCli,
    pub(crate) clients:
        std::sync::Arc<dyn crate::exec_tool::work_queue_tools::WorkQueueGithubClients>,
    pub(crate) terminals:
        tddy_session_lifecycle::terminal_session_adapter::DaemonTerminalSessionStore,
}

impl GitEnvSource {
    pub(crate) async fn env_of(
        &self,
        store: tddy_core::work_queue_store::WorkQueueStore,
        sessions_base: &std::path::Path,
        os_user: &str,
        address: &tddy_service::proto::work_queue::WorkQueueAddress,
    ) -> Result<GitEnv, Status> {
        let queue = store
            .read(&address.project_id, &address.queue_id)
            .map(|record| record.stack)
            .map_err(super::project_stacks::store_status)?;
        let live = self.terminals.live_sessions().await;
        Ok(GitEnv {
            projects_dir: store.projects_dir().to_path_buf(),
            records: records_of(sessions_base, &queue, &live),
            store,
            project_id: address.project_id.clone(),
            queue_id: address.queue_id.clone(),
            os_user: os_user.to_string(),
            identity: self.identity.clone(),
            github_cli: self.github_cli.clone(),
            clients: std::sync::Arc::clone(&self.clients),
        })
    }
}

/// Hand every met schedule of `views` to a runner on a task of its own, so whoever evaluated them is
/// not held up by git. The store write that takes a schedule out is what makes the firing happen
/// once, however many evaluations see it met.
pub(crate) fn fire_met_in_background(
    fire: DaemonFireSchedule,
    views: Vec<tddy_work_queue::schedule::ScheduleView>,
) {
    use tddy_work_queue::schedule::ConditionState;
    if !views.iter().any(|view| view.status == ConditionState::Met) {
        return;
    }
    tokio::spawn(async move {
        let runner =
            tddy_work_queue::schedule_runner::ScheduleRunner::new(std::sync::Arc::new(fire));
        let fired = runner.on_change(&views).await;
        if !fired.is_empty() {
            log::info!("fired armed schedules of {fired:?}");
        }
    });
}

/// What firing one claimed schedule does: land (merge) or propagate the item.
#[async_trait::async_trait]
pub(crate) trait FireAction: Send + Sync {
    async fn run(
        &self,
        item_id: &str,
        kind: &tddy_work_queue::schedule::ScheduleKind,
    ) -> Result<(), String>;
}

/// The engines, over this host's git and GitHub.
pub(crate) struct EngineAction {
    pub(crate) source: GitEnvSource,
    pub(crate) store: tddy_core::work_queue_store::WorkQueueStore,
    pub(crate) sessions_base: std::path::PathBuf,
    pub(crate) os_user: String,
    pub(crate) address: tddy_service::proto::work_queue::WorkQueueAddress,
}

#[async_trait::async_trait]
impl FireAction for EngineAction {
    async fn run(
        &self,
        item_id: &str,
        kind: &tddy_work_queue::schedule::ScheduleKind,
    ) -> Result<(), String> {
        use tddy_work_queue::schedule::ScheduleKind;
        let env = self
            .source
            .env_of(
                self.store.clone(),
                &self.sessions_base,
                &self.os_user,
                &self.address,
            )
            .await
            .map_err(|status| status.message().to_string())?;
        let (kind, item_id) = (kind.clone(), item_id.to_string());
        tokio::task::spawn_blocking(move || match kind {
            ScheduleKind::Merge { .. } => land_ops::merge_item(&env, &item_id).map(drop),
            ScheduleKind::Propagate => land_ops::propagate_item(&env, &item_id, None, false)
                .map(drop)
                .map_err(|refusal| format!("{item_id} was not propagated — {refusal}")),
        })
        .await
        .map_err(|e| format!("the firing failed to finish: {e}"))?
    }
}

/// Fires an armed schedule: the schedule is taken out of the store first, in one write, so a second
/// firing of the same arming finds it gone and does nothing; then the action runs.
pub(crate) struct DaemonFireSchedule {
    pub(crate) store: tddy_core::work_queue_store::WorkQueueStore,
    pub(crate) address: tddy_service::proto::work_queue::WorkQueueAddress,
    pub(crate) action: std::sync::Arc<dyn FireAction>,
}

impl DaemonFireSchedule {
    /// Firing through the engines.
    pub(crate) fn through_engines(
        source: GitEnvSource,
        store: tddy_core::work_queue_store::WorkQueueStore,
        sessions_base: std::path::PathBuf,
        os_user: String,
        address: tddy_service::proto::work_queue::WorkQueueAddress,
    ) -> Self {
        Self {
            action: std::sync::Arc::new(EngineAction {
                source,
                store: store.clone(),
                sessions_base,
                os_user,
                address: address.clone(),
            }),
            store,
            address,
        }
    }
}

#[async_trait::async_trait]
impl tddy_work_queue::schedule_runner::FireSchedule for DaemonFireSchedule {
    async fn fire(&self, view: &tddy_work_queue::schedule::ScheduleView) -> Result<(), String> {
        let (item_id, armed_at) = (
            view.schedule.item_id.clone(),
            view.schedule.armed_at.clone(),
        );
        let mut taken = false;
        self.store
            .update(&self.address.project_id, &self.address.queue_id, |queue| {
                let before = queue.schedules.len();
                queue
                    .schedules
                    .retain(|s| !(s.item_id == item_id && s.armed_at == armed_at));
                taken = queue.schedules.len() < before;
            })
            .map_err(|e| e.to_string())?;
        if !taken {
            return Ok(());
        }
        self.action.run(&item_id, &view.schedule.kind).await
    }
}

impl WorkQueueRpcHandler {
    pub(super) fn git_env_source(&self, session_token: &str) -> GitEnvSource {
        GitEnvSource {
            identity: SessionAccountAccess::new(
                self.credential_vaults.clone(),
                self.user_resolver.clone(),
                session_token,
            ),
            github_cli: self.github_cli.clone(),
            clients: std::sync::Arc::new(RealWorkQueueGithubClients),
            terminals: self.terminals.clone(),
        }
    }

    /// After a schedule was armed or removed: fire what is already met, and keep a watcher on the
    /// repository's inbox until nothing is armed any more.
    pub(super) async fn fire_met_schedules(&self, call: &LinkCall, session_token: &str) {
        let Ok(sessions_base) = self.sessions_base(&call.os_user) else {
            return;
        };
        super::schedule_watch::after_arming(super::schedule_watch::WatchInputs {
            source: self.git_env_source(session_token),
            pull_requests: std::sync::Arc::clone(&self.pull_requests),
            store: self.stack_store(&sessions_base),
            sessions_base,
            os_user: call.os_user.clone(),
            address: call.address.clone(),
            session_token: session_token.to_string(),
        })
        .await;
    }

    async fn git_env_of(&self, call: &LinkCall, session_token: &str) -> Result<GitEnv, Status> {
        let sessions_base = self.sessions_base(&call.os_user)?;
        self.git_env_source(session_token)
            .env_of(
                self.stack_store(&sessions_base),
                &sessions_base,
                &call.os_user,
                &call.address,
            )
            .await
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use pretty_assertions::assert_eq;
    use tddy_changeset::work_queue_fixtures::{a_stack_host, A_PROJECT};
    use tddy_core::changeset::{WorkItem, WorkQueue};
    use tddy_work_queue::schedule::{ConditionState, Schedule, ScheduleKind, ScheduleView};
    use tddy_work_queue::schedule_runner::ScheduleRunner;

    use super::*;

    /// The engines' port: records what would have been merged or propagated.
    #[derive(Default)]
    struct ARecordingEngine {
        ran: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl FireAction for ARecordingEngine {
        async fn run(&self, item_id: &str, kind: &ScheduleKind) -> Result<(), String> {
            let what = match kind {
                ScheduleKind::Merge { .. } => "merge",
                ScheduleKind::Propagate => "propagate",
            };
            self.ran.lock().unwrap().push(format!("{what} {item_id}"));
            Ok(())
        }
    }

    fn an_armed(item: &str, kind: ScheduleKind) -> Schedule {
        Schedule {
            item_id: item.to_string(),
            kind,
            armed_at: "2026-11-23T10:00:00Z".to_string(),
            armed_by: "octocat".to_string(),
        }
    }

    fn a_view(schedule: &Schedule, status: ConditionState) -> ScheduleView {
        ScheduleView {
            schedule: schedule.clone(),
            conditions: Vec::new(),
            status,
            held_up_on: None,
        }
    }

    #[tokio::test]
    async fn a_met_armed_merge_runs_the_engine_once_and_is_disarmed_in_the_store() {
        // Given a queue with n1 armed to merge and n2 armed to propagate
        let host = a_stack_host();
        let merge = an_armed("n1", ScheduleKind::Merge { skip_review: false });
        let propagate = an_armed("n2", ScheduleKind::Propagate);
        let queue = WorkQueue {
            version: 1,
            nodes: ["n1", "n2"]
                .map(|id| WorkItem {
                    node_id: id.to_string(),
                    ..WorkItem::default()
                })
                .to_vec(),
            schedules: vec![merge.clone(), propagate.clone()],
        };
        host.orchestrator("owner", queue);
        let store = tddy_core::work_queue_store::WorkQueueStore::new(host.base());
        let engine = Arc::new(ARecordingEngine::default());
        let runner = ScheduleRunner::new(Arc::new(DaemonFireSchedule {
            store: store.clone(),
            address: tddy_service::proto::work_queue::WorkQueueAddress {
                project_id: A_PROJECT.to_string(),
                queue_id: "owner".to_string(),
                daemon_instance_id: String::new(),
            },
            action: engine.clone(),
        }));

        // When n1's last condition turns met, the event is seen twice by two evaluations, and n2
        // is still waiting
        let first = runner
            .on_change(&[
                a_view(&merge, ConditionState::Met),
                a_view(&propagate, ConditionState::Waiting),
            ])
            .await;
        let again = runner
            .on_change(&[a_view(&merge, ConditionState::Met)])
            .await;

        // Then the engine merged n1 once, n1 is no longer armed in the store, n2 still is
        let armed: Vec<String> = store
            .read(A_PROJECT, "owner")
            .unwrap()
            .stack
            .schedules
            .into_iter()
            .map(|s| s.item_id)
            .collect();
        assert_eq!(
            (first, again, engine.ran.lock().unwrap().clone(), armed),
            (
                vec!["n1".to_string()],
                Vec::<String>::new(),
                vec!["merge n1".to_string()],
                vec!["n2".to_string()]
            )
        );
    }

    #[tokio::test]
    async fn two_runners_seeing_the_same_arming_met_run_the_engine_once() {
        // Given one armed merge and two watchers (two runners over the same store)
        let host = a_stack_host();
        let merge = an_armed("n1", ScheduleKind::Merge { skip_review: false });
        host.orchestrator(
            "owner",
            WorkQueue {
                version: 1,
                nodes: vec![WorkItem {
                    node_id: "n1".to_string(),
                    ..WorkItem::default()
                }],
                schedules: vec![merge.clone()],
            },
        );
        let store = tddy_core::work_queue_store::WorkQueueStore::new(host.base());
        let engine = Arc::new(ARecordingEngine::default());
        let runner = || {
            ScheduleRunner::new(Arc::new(DaemonFireSchedule {
                store: store.clone(),
                address: tddy_service::proto::work_queue::WorkQueueAddress {
                    project_id: A_PROJECT.to_string(),
                    queue_id: "owner".to_string(),
                    daemon_instance_id: String::new(),
                },
                action: engine.clone(),
            }))
        };

        // When both see it met
        runner()
            .on_change(&[a_view(&merge, ConditionState::Met)])
            .await;
        runner()
            .on_change(&[a_view(&merge, ConditionState::Met)])
            .await;

        // Then the engine ran once
        assert_eq!(
            engine.ran.lock().unwrap().clone(),
            vec!["merge n1".to_string()]
        );
    }
}
