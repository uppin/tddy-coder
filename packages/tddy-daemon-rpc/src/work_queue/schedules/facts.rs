//! The facts a schedule is judged on, each read from the surface that owns it.

use super::*;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_github::pr_api::{CheckStatus, PrDetail, PrState};
use tddy_service::proto::work_queue::{ScheduleConditionView, ScheduleStatusView};
use tddy_work_queue::commit_flow::{InboxEntry, Prediction};
use tddy_work_queue::inbox_reads::inbox_error_reason;
use tddy_work_queue::schedule::{
    self, checks_verdict, ChecksVerdict, Condition, ConditionState, RunOutcome, Schedule,
    ScheduleKind, ScheduleView, WorktreeVerdict,
};
use tddy_worktree_service::pull_request_view::PullRequestSite;
use tddy_worktree_service::worktree_sync::{read_sync_state, SyncStatus};

pub(super) fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    schedule::iso8601_utc(secs)
}

pub(super) fn kind_of(kind: &str, skip_review: bool) -> Result<ScheduleKind, ScheduleOpError> {
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

pub(super) fn state_name(state: ConditionState) -> &'static str {
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
pub(super) fn unreadable(schedule: Schedule, reason: String) -> ScheduleView {
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

pub(super) fn checks_of(runs: &[tddy_github::pr_api::CheckRun]) -> ChecksVerdict {
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

pub(super) fn worktree_of(
    item: &WorkItem,
    sessions_base: &Path,
) -> Result<WorktreeVerdict, String> {
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
pub(super) async fn item_pr(
    site: &PullRequestSite,
    item: &WorkItem,
) -> Result<Option<PrDetail>, String> {
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

pub(super) fn is_open(pr: &PrDetail) -> bool {
    matches!(pr.state, PrState::Open | PrState::Draft)
}

/// `pr`'s checks at its head.
pub(super) async fn checks_of_pr(
    site: &PullRequestSite,
    pr: &PrDetail,
) -> Result<ChecksVerdict, String> {
    let runs = site
        .inbox
        .checks_at(&site.repo, &pr.head_sha)
        .await
        .map_err(|e| inbox_error_reason(&e))?;
    Ok(checks_of(&runs.value))
}

/// The inbox counts `(clean, conflicting)` of `entries`.
pub(super) fn inbox_counts(entries: &[InboxEntry]) -> (u32, u32) {
    let conflicting = entries
        .iter()
        .filter(|e| matches!(e.prediction, Prediction::Conflicts(_)))
        .count() as u32;
    (entries.len() as u32 - conflicting, conflicting)
}
