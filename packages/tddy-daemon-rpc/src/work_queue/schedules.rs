//! `ScheduleWorkItem`, `ScheduleWorkQueueWave`, `UnscheduleWorkItem` and `ListWorkQueueSchedules`
//! (`#workqueue` 13/14): arm a merge or a propagation on an item, arm a wave's merges bottom-up,
//! disarm, and read every armed schedule with its conditions evaluated live.
//!
//! The facts a schedule is judged on are gathered here, each from the surface that owns it: the line
//! parent (`lineage`), the inbox and its conflict predictions (`commit_flow`), checks and reviews (the
//! GitHub inbox, rows other readers share), and the worktree's sync state. A fact that cannot be read
//! never reads as met: the schedule is shown `waiting` on `facts`, with the reason. Nothing here fires
//! a schedule (`#workqueue` 14/14).

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_github::pr_api::{CheckStatus, PrDetail, PrState};
use tddy_github_inbox::InboxError;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    ListWorkQueueSchedulesRequest, ListWorkQueueSchedulesResponse, ScheduleConditionView,
    ScheduleStatusView, ScheduleWorkItemRequest, ScheduleWorkItemResponse,
    ScheduleWorkQueueWaveRequest, UnscheduleWorkItemRequest,
};
use tddy_work_queue::commit_flow::{InboxEntry, Prediction};
use tddy_work_queue::inbox_reads::inbox_error_reason;
use tddy_work_queue::lineage::{self, LineEndpoint};
use tddy_work_queue::schedule::{
    self, checks_verdict, review_verdict, ChecksVerdict, Condition, ConditionState, ReviewVerdict,
    RunOutcome, Schedule, ScheduleError, ScheduleKind, ScheduleSnapshot, ScheduleView,
    WorktreeVerdict,
};
use tddy_work_queue::waves;
use tddy_worktree_service::pull_request_view::{PullRequestSite, SessionPullRequests};
use tddy_worktree_service::worktree_sync::{read_sync_state, SyncStatus};

use super::commit_flow::{flow_site, read_flow, FlowInputs};
use super::project_stacks::store_status;
use super::WorkQueueRpcHandler;
use tddy_core::work_queue_store::WorkQueueStore;

/// Why a schedule operation was refused.
#[derive(Debug)]
pub(crate) enum ScheduleOpError {
    NotFound(String),
    Invalid(String),
    Failed(String),
}

impl ScheduleOpError {
    pub(crate) fn into_status(self) -> Status {
        match self {
            Self::NotFound(m) => Status::not_found(m),
            Self::Invalid(m) => Status::invalid_argument(m),
            Self::Failed(m) => Status::failed_precondition(m),
        }
    }
}

impl std::fmt::Display for ScheduleOpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(m) | Self::Invalid(m) | Self::Failed(m) => f.write_str(m),
        }
    }
}

impl From<ScheduleError> for ScheduleOpError {
    fn from(error: ScheduleError) -> Self {
        Self::NotFound(error.to_string())
    }
}

fn store_failure(error: tddy_core::work_queue_store::WorkQueueStoreError) -> ScheduleOpError {
    let status = store_status(error);
    ScheduleOpError::Failed(status.message().to_string())
}

/// Everything a schedule operation runs over, for one queue on this host.
pub(crate) struct ScheduleCtx<'a> {
    pub store: &'a WorkQueueStore,
    pub pull_requests: &'a dyn SessionPullRequests,
    pub os_user: &'a str,
    pub sessions_base: &'a Path,
    pub project_id: &'a str,
    pub queue_id: &'a str,
    /// The session the GitHub site is registered for.
    pub caller_session_id: &'a str,
    pub session_token: &'a str,
    pub trunk: &'a str,
    /// Who arms: stamped as `armed_by`.
    pub armed_by: &'a str,
}

fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    schedule::iso8601_utc(secs)
}

fn kind_of(kind: &str, skip_review: bool) -> Result<ScheduleKind, ScheduleOpError> {
    match kind.trim() {
        "merge" => Ok(ScheduleKind::Merge { skip_review }),
        "propagate" if skip_review => Err(ScheduleOpError::Invalid(
            "skip_review applies to a merge only".to_string(),
        )),
        "propagate" => Ok(ScheduleKind::Propagate),
        other => Err(ScheduleOpError::Invalid(format!(
            "kind must be 'merge' or 'propagate', not '{other}'"
        ))),
    }
}

fn state_name(state: ConditionState) -> &'static str {
    match state {
        ConditionState::Met => "met",
        ConditionState::Waiting => "waiting",
        ConditionState::Blocked => "blocked",
    }
}

/// The wire form of an evaluated schedule.
pub(crate) fn status_view(view: ScheduleView) -> ScheduleStatusView {
    let (kind, skip_review) = match view.schedule.kind {
        ScheduleKind::Merge { skip_review } => ("merge", skip_review),
        ScheduleKind::Propagate => ("propagate", false),
    };
    ScheduleStatusView {
        item_id: view.schedule.item_id,
        kind: kind.to_string(),
        skip_review,
        armed_at: view.schedule.armed_at,
        armed_by: view.schedule.armed_by,
        status: state_name(view.status).to_string(),
        held_up_on: view.held_up_on.unwrap_or_default().to_string(),
        conditions: view
            .conditions
            .into_iter()
            .map(|c| ScheduleConditionView {
                name: c.name.to_string(),
                state: state_name(c.state).to_string(),
                detail: c.detail,
            })
            .collect(),
    }
}

/// A schedule whose facts could not be gathered: waiting on `facts`, with why.
fn unreadable(schedule: Schedule, reason: String) -> ScheduleView {
    ScheduleView {
        schedule,
        conditions: vec![Condition {
            name: "facts",
            state: ConditionState::Waiting,
            detail: reason,
        }],
        status: ConditionState::Waiting,
        held_up_on: Some("facts"),
    }
}

/// Every armed schedule of `queue` shown waiting on `facts`, for when nothing could be read.
pub(crate) fn unreadable_views(queue: &WorkQueue, reason: &str) -> Vec<ScheduleStatusView> {
    queue
        .schedules
        .iter()
        .map(|armed| status_view(unreadable(armed.clone(), reason.to_string())))
        .collect()
}

fn checks_of(runs: &[tddy_github::pr_api::CheckRun]) -> ChecksVerdict {
    let outcomes: Vec<RunOutcome> = runs
        .iter()
        .map(|run| match run.status {
            CheckStatus::Queued | CheckStatus::InProgress => RunOutcome::Running,
            CheckStatus::Completed
                if matches!(run.conclusion.as_str(), "success" | "neutral" | "skipped") =>
            {
                RunOutcome::Passed
            }
            CheckStatus::Completed => RunOutcome::Failed,
        })
        .collect();
    checks_verdict(&outcomes)
}

fn worktree_of(item: &WorkItem, sessions_base: &Path) -> Result<WorktreeVerdict, String> {
    let session_id = item
        .session_id
        .as_deref()
        .ok_or_else(|| "the item has no session, so no worktree to check".to_string())?;
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    let worktree = tddy_core::read_session_metadata(&session_dir)
        .map_err(|e| format!("session metadata: {e}"))?
        .repo_path
        .ok_or_else(|| "the item's session has no worktree".to_string())?;
    let state = read_sync_state(Path::new(&worktree)).map_err(|e| e.to_string())?;
    Ok(if !state.dirty_files.is_empty() {
        WorktreeVerdict::Dirty
    } else if state.status == SyncStatus::InSync {
        WorktreeVerdict::Clean
    } else {
        WorktreeVerdict::Unpushed
    })
}

/// The PR an item stands on: its assigned one, else its branch's. `None` without a branch or PR.
async fn item_pr(site: &PullRequestSite, item: &WorkItem) -> Result<Option<PrDetail>, String> {
    let read = match (&item.pr, item.branch.as_deref()) {
        (Some(assigned), _) => site
            .inbox
            .pr(&site.repo, assigned.number)
            .await
            .map(|c| Some(c.value)),
        (None, Some(branch)) => site
            .inbox
            .pr_for_head(&site.repo, branch)
            .await
            .map(|c| c.value),
        (None, None) => return Ok(None),
    };
    read.map_err(|e| inbox_error_reason(&e))
}

fn is_open(pr: &PrDetail) -> bool {
    matches!(pr.state, PrState::Open | PrState::Draft)
}

/// `pr`'s checks at its head.
async fn checks_of_pr(site: &PullRequestSite, pr: &PrDetail) -> Result<ChecksVerdict, String> {
    let runs = site
        .inbox
        .checks_at(&site.repo, &pr.head_sha)
        .await
        .map_err(|e| inbox_error_reason(&e))?;
    Ok(checks_of(&runs.value))
}

/// The inbox counts `(clean, conflicting)` of `entries`.
fn inbox_counts(entries: &[InboxEntry]) -> (u32, u32) {
    let conflicting = entries
        .iter()
        .filter(|e| matches!(e.prediction, Prediction::Conflicts(_)))
        .count() as u32;
    (entries.len() as u32 - conflicting, conflicting)
}

impl ScheduleCtx<'_> {
    fn queue(&self) -> Result<WorkQueue, ScheduleOpError> {
        self.store
            .read(self.project_id, self.queue_id)
            .map(|record| record.stack)
            .map_err(store_failure)
    }

    fn update(
        &self,
        edit: impl FnOnce(&mut WorkQueue) -> Result<(), ScheduleError>,
    ) -> Result<WorkQueue, ScheduleOpError> {
        let mut refused = None;
        let record = self
            .store
            .update(self.project_id, self.queue_id, |queue| {
                let mut edited = queue.clone();
                match edit(&mut edited) {
                    Ok(()) => *queue = edited,
                    Err(error) => refused = Some(error),
                }
            })
            .map_err(store_failure)?;
        match refused {
            Some(error) => Err(error.into()),
            None => Ok(record.stack),
        }
    }

    fn site(&self) -> Result<PullRequestSite, String> {
        self.pull_requests
            .site_for_session(
                self.os_user,
                self.caller_session_id,
                self.project_id,
                self.session_token,
            )
            .map_err(|status| status.message().to_string())
    }

    /// Arm `kind` on `item_id`, answering the schedule as first evaluated.
    pub(crate) async fn arm_item(
        &self,
        item_id: &str,
        kind: &str,
        skip_review: bool,
    ) -> Result<ScheduleStatusView, ScheduleOpError> {
        let kind = kind_of(kind, skip_review)?;
        let (armed_at, armed_by) = (now_iso(), self.armed_by.to_string());
        let item_id = item_id.trim().to_string();
        let queue = self.update(|queue| {
            schedule::arm(queue, &item_id, kind, &armed_at, &armed_by).map(|_| ())
        })?;
        let (views, _) = self.evaluate(&queue).await;
        views
            .into_iter()
            .find(|v| v.item_id == item_id)
            .ok_or_else(|| ScheduleOpError::Failed("the armed schedule was not stored".to_string()))
    }

    /// Arm a merge on every item of `wave` that has an open PR, bottom-up.
    pub(crate) async fn arm_wave(
        &self,
        wave: u32,
        skip_review: bool,
    ) -> Result<(Vec<ScheduleStatusView>, String), ScheduleOpError> {
        let queue = self.queue()?;
        let items: Vec<String> = queue.nodes.iter().map(|n| n.node_id.clone()).collect();
        // Wave membership is structural: what has already landed does not change it.
        let derived = waves::queue_waves(&items, &lineage::dependencies_of(&queue), &[])
            .map_err(|e| ScheduleOpError::Failed(e.to_string()))?;
        let in_wave = usize::try_from(wave)
            .ok()
            .and_then(|w| w.checked_sub(1))
            .and_then(|index| derived.waves.get(index))
            .ok_or_else(|| {
                ScheduleOpError::NotFound(format!(
                    "wave {wave} does not exist (the queue has {})",
                    derived.waves.len()
                ))
            })?;
        let bottom_up: Vec<String> = derived
            .derived_line
            .iter()
            .filter(|id| in_wave.contains(id))
            .cloned()
            .collect();
        let site = self.site().map_err(|reason| {
            ScheduleOpError::Failed(format!(
                "which items have an open pull request could not be read: {reason}"
            ))
        })?;
        let mut open: HashMap<String, bool> = HashMap::new();
        for id in &bottom_up {
            let item = queue.node(id).expect("a wave holds the queue's own items");
            let has_open = item_pr(&site, item)
                .await
                .map_err(|reason| {
                    ScheduleOpError::Failed(format!(
                        "the pull request of {id} could not be read: {reason}"
                    ))
                })?
                .is_some_and(|pr| is_open(&pr));
            open.insert(id.clone(), has_open);
        }
        let (armed_at, armed_by) = (now_iso(), self.armed_by.to_string());
        let queue = self.update(|queue| {
            schedule::arm_wave(
                queue,
                &bottom_up,
                |id| open.get(id).copied().unwrap_or(false),
                skip_review,
                &armed_at,
                &armed_by,
            )
            .map(|_| ())
        })?;
        let (views, reason) = self.evaluate(&queue).await;
        Ok((views, reason))
    }

    pub(crate) async fn disarm(
        &self,
        item_id: &str,
    ) -> Result<(Vec<ScheduleStatusView>, String), ScheduleOpError> {
        let item_id = item_id.trim().to_string();
        let queue = self.update(|queue| schedule::disarm(queue, &item_id))?;
        Ok(self.evaluate(&queue).await)
    }

    pub(crate) async fn list(&self) -> Result<(Vec<ScheduleStatusView>, String), ScheduleOpError> {
        let queue = self.queue()?;
        Ok(self.evaluate(&queue).await)
    }

    /// Every armed schedule of `queue`, evaluated now, in the order armed; and why GitHub or the
    /// checkout could not be read at all (empty when they could).
    pub(crate) async fn evaluate(&self, queue: &WorkQueue) -> (Vec<ScheduleStatusView>, String) {
        if queue.schedules.is_empty() {
            return (Vec::new(), String::new());
        }
        let site = self.site();
        let line = match &site {
            Ok(site) => {
                lineage::queue_graph(&site.inbox, &site.repo, queue, self.trunk)
                    .await
                    .line
            }
            Err(_) => lineage::planned_line_edges(queue, self.trunk),
        };
        let flow = flow_site(FlowInputs {
            pull_requests: self.pull_requests,
            os_user: self.os_user,
            sessions_base: self.sessions_base,
            queue_id: self.queue_id,
            caller_session_id: self.caller_session_id,
            project_id: self.project_id,
            session_token: self.session_token,
            queue,
            trunk: self.trunk,
        })
        .await;
        let wanted: Vec<String> = queue.schedules.iter().map(|s| s.item_id.clone()).collect();
        let inboxes: Result<HashMap<String, Result<Vec<InboxEntry>, String>>, String> = match &flow
        {
            Ok(flow) => {
                read_flow(flow, move |f, items| {
                    Ok(wanted
                        .into_iter()
                        .map(|id| {
                            let entries = f.inbox(items, &id).map_err(|e| e.to_string());
                            (id, entries)
                        })
                        .collect())
                })
                .await
            }
            Err(reason) => Err(reason.clone()),
        };
        let mut views = Vec::new();
        for armed in &queue.schedules {
            let facts = self
                .snapshot_of(queue, armed, &line, site.as_ref(), inboxes.as_ref())
                .await;
            views.push(match facts {
                Ok(snapshot) => schedule::evaluate(armed, &snapshot),
                Err(reason) => unreadable(armed.clone(), reason),
            });
        }
        let reason = match (&site, &flow) {
            (Err(reason), _) => reason.clone(),
            (_, Err(reason)) => reason.clone(),
            _ => String::new(),
        };
        (views.into_iter().map(status_view).collect(), reason)
    }

    /// The facts of one armed schedule; the first that cannot be read is the error.
    async fn snapshot_of(
        &self,
        queue: &WorkQueue,
        armed: &Schedule,
        line: &[(String, LineEndpoint)],
        site: Result<&PullRequestSite, &String>,
        inboxes: Result<&HashMap<String, Result<Vec<InboxEntry>, String>>, &String>,
    ) -> Result<ScheduleSnapshot, String> {
        let item = queue
            .node(&armed.item_id)
            .ok_or_else(|| format!("item '{}' no longer exists", armed.item_id))?;
        let base = line
            .iter()
            .find(|(id, _)| *id == armed.item_id)
            .map(|(_, endpoint)| endpoint)
            .ok_or_else(|| "the item has no readable line parent".to_string())?;
        let entries = inboxes
            .map_err(|reason| format!("the inbox could not be read: {reason}"))?
            .get(&armed.item_id)
            .ok_or_else(|| "the inbox was not read for this item".to_string())?
            .as_ref()
            .map_err(|reason| format!("the inbox could not be read: {reason}"))?;
        let (inbox_clean, inbox_conflicting) = inbox_counts(entries);
        let site = site.map_err(|reason| format!("GitHub could not be read: {reason}"))?;

        let merge = matches!(armed.kind, ScheduleKind::Merge { .. });
        let (own_checks, review) = if merge {
            let pr = item_pr(site, item)
                .await?
                .ok_or_else(|| "the item has no pull request".to_string())?;
            if !is_open(&pr) {
                return Err("the item's pull request is no longer open".to_string());
            }
            let checks = checks_of_pr(site, &pr).await?;
            let conversation = site
                .inbox
                .conversation(&site.repo, pr.number)
                .await
                .map_err(|e: InboxError| inbox_error_reason(&e))?;
            let review = review_verdict(
                conversation
                    .value
                    .reviews
                    .iter()
                    .map(|r| (r.author.as_str(), r.state.as_str())),
            );
            (checks, review)
        } else {
            (ChecksVerdict::NotOurs, ReviewVerdict::Pending)
        };
        let base_checks = match (merge, base) {
            (false, LineEndpoint::Item(base_id)) => {
                let base_item = queue
                    .node(base_id)
                    .ok_or_else(|| format!("the base item '{base_id}' no longer exists"))?;
                match item_pr(site, base_item).await? {
                    Some(pr) => checks_of_pr(site, &pr).await?,
                    None => ChecksVerdict::NotOurs,
                }
            }
            _ => ChecksVerdict::NotOurs,
        };
        let worktree = if merge {
            WorktreeVerdict::Clean
        } else {
            worktree_of(item, self.sessions_base)?
        };
        Ok(ScheduleSnapshot {
            base_is_trunk: matches!(base, LineEndpoint::Trunk(_)),
            inbox_clean,
            inbox_conflicting,
            own_checks,
            base_checks,
            review,
            worktree,
        })
    }
}

/// The tools' answers, as the wire's views in JSON.
pub(crate) mod json {
    use super::*;

    pub(crate) fn schedule(view: ScheduleStatusView) -> Value {
        json!({
            "item_id": view.item_id,
            "kind": view.kind,
            "skip_review": view.skip_review,
            "armed_at": view.armed_at,
            "armed_by": view.armed_by,
            "status": view.status,
            "held_up_on": view.held_up_on,
            "conditions": view.conditions.into_iter().map(|c| json!({
                "name": c.name, "state": c.state, "detail": c.detail,
            })).collect::<Vec<_>>(),
        })
    }

    pub(crate) fn schedules(views: Vec<ScheduleStatusView>, unavailable_reason: &str) -> Value {
        json!({
            "schedules": views.into_iter().map(schedule).collect::<Vec<_>>(),
            "unavailable_reason": unavailable_reason,
        })
    }
}

fn listed(
    (schedules, unavailable_reason): (Vec<ScheduleStatusView>, String),
) -> Response<ListWorkQueueSchedulesResponse> {
    Response::new(ListWorkQueueSchedulesResponse {
        schedules,
        unavailable_reason,
    })
}

impl WorkQueueRpcHandler {
    /// Resolve the call's queue on this host and run `act` over it; `None` when the address names
    /// another host (the caller forwards).
    async fn with_schedules<T>(
        &self,
        session_token: &str,
        queue: Option<&tddy_service::proto::work_queue::WorkQueueAddress>,
        act: impl for<'a> FnOnce(
            ScheduleCtx<'a>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, ScheduleOpError>> + Send + 'a>,
        >,
    ) -> Result<T, Status> {
        let call = self.link_call(session_token, queue)?;
        let (sessions_base, _, trunk) = self.read_queue_and_trunk(&call)?;
        let store = self.stack_store(&sessions_base);
        let ctx = ScheduleCtx {
            store: &store,
            pull_requests: self.pull_requests.as_ref(),
            os_user: &call.os_user,
            sessions_base: &sessions_base,
            project_id: &call.address.project_id,
            queue_id: &call.address.queue_id,
            caller_session_id: &call.address.queue_id,
            session_token,
            trunk: &trunk,
            armed_by: &call.os_user,
        };
        act(ctx).await.map_err(ScheduleOpError::into_status)
    }

    pub(super) async fn schedule_work_item_rpc(
        &self,
        request: Request<ScheduleWorkItemRequest>,
    ) -> Result<Response<ScheduleWorkItemResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, ScheduleWorkItemResponse>(
                "ScheduleWorkItem",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let (item_id, kind, skip_review) = (req.item_id.clone(), req.kind.clone(), req.skip_review);
        let schedule = self
            .with_schedules(&req.session_token, req.queue.as_ref(), move |ctx| {
                Box::pin(async move { ctx.arm_item(&item_id, &kind, skip_review).await })
            })
            .await?;
        Ok(Response::new(ScheduleWorkItemResponse {
            schedule: Some(schedule),
        }))
    }

    pub(super) async fn schedule_work_queue_wave_rpc(
        &self,
        request: Request<ScheduleWorkQueueWaveRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, ListWorkQueueSchedulesResponse>(
                "ScheduleWorkQueueWave",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let (wave, skip_review) = (req.wave, req.skip_review);
        let answered = self
            .with_schedules(&req.session_token, req.queue.as_ref(), move |ctx| {
                Box::pin(async move { ctx.arm_wave(wave, skip_review).await })
            })
            .await?;
        Ok(listed(answered))
    }

    pub(super) async fn unschedule_work_item_rpc(
        &self,
        request: Request<UnscheduleWorkItemRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, ListWorkQueueSchedulesResponse>(
                "UnscheduleWorkItem",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let item_id = req.item_id.clone();
        let answered = self
            .with_schedules(&req.session_token, req.queue.as_ref(), move |ctx| {
                Box::pin(async move { ctx.disarm(&item_id).await })
            })
            .await?;
        Ok(listed(answered))
    }

    pub(super) async fn list_work_queue_schedules_rpc(
        &self,
        request: Request<ListWorkQueueSchedulesRequest>,
    ) -> Result<Response<ListWorkQueueSchedulesResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, ListWorkQueueSchedulesResponse>(
                "ListWorkQueueSchedules",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let answered = self
            .with_schedules(&req.session_token, req.queue.as_ref(), move |ctx| {
                Box::pin(async move { ctx.list().await })
            })
            .await?;
        Ok(listed(answered))
    }
}
