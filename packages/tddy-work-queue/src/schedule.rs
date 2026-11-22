//! Armed merges and propagations (`#workqueue` 13/14). A schedule arms a **merge** or a **propagation**
//! on a work item; this module only *evaluates* its conditions against a snapshot of the item's facts —
//! firing is `#workqueue` 14/14's. Pure: no clock (`armed_at` is stamped by the caller), no I/O.
//!
//! - Merge: `order` (its base is the trunk), `current` (inbox empty — clean commits wait, a predicted
//!   conflict blocks), `checks` (its own CI), `review` (approved; `skip_review` stands in for an
//!   approval that has not arrived once CI is green, never for a change request).
//! - Propagate: `inbox` (something to take), `conflicts` (none predicted), `checks` (the **base's** CI —
//!   a propagation replays the base's commits; an external base has no CI of ours), `worktree` (nothing
//!   uncommitted or unpushed).
//!
//! Status is the worst condition (blocked > waiting > met); `held_up_on` is the first blocked condition,
//! else the first waiting one. No `threads` condition: resolved review-thread state is GraphQL-only
//! (operator, 2026-11-22: a recorded TODO).

use tddy_core::changeset::WorkQueue;

/// A schedule as stored on the queue (`tddy_changeset`'s), re-exported as the evaluator's input.
pub use tddy_core::changeset::{Schedule, ScheduleKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConditionState {
    Met,
    /// Will hold by itself.
    Waiting,
    /// Needs a person.
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub name: &'static str,
    pub state: ConditionState,
    /// What a person reads.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleView {
    pub schedule: Schedule,
    pub conditions: Vec<Condition>,
    /// The worst condition's state.
    pub status: ConditionState,
    /// The first blocked condition's name, else the first waiting one's.
    pub held_up_on: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksVerdict {
    Green,
    Pending,
    Failed,
    /// No CI of ours (an external base).
    NotOurs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    Approved,
    ChangesRequested,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeVerdict {
    Clean,
    Dirty,
    Unpushed,
}

/// The facts a schedule is evaluated against, gathered from n9 (line), n11 (inbox), n2 (checks,
/// reviews) and n4 (worktree sync state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleSnapshot {
    pub base_is_trunk: bool,
    pub inbox_clean: u32,
    pub inbox_conflicting: u32,
    pub own_checks: ChecksVerdict,
    pub base_checks: ChecksVerdict,
    pub review: ReviewVerdict,
    pub worktree: WorktreeVerdict,
}

fn condition(name: &'static str, state: ConditionState, detail: impl Into<String>) -> Condition {
    Condition {
        name,
        state,
        detail: detail.into(),
    }
}

fn checks_condition(verdict: ChecksVerdict, whose: &str) -> Condition {
    let (state, detail) = match verdict {
        ChecksVerdict::Green => (ConditionState::Met, format!("{whose} checks are green")),
        ChecksVerdict::Pending => (
            ConditionState::Waiting,
            format!("{whose} checks are running"),
        ),
        ChecksVerdict::Failed => (ConditionState::Blocked, format!("{whose} checks failed")),
        ChecksVerdict::NotOurs => (
            ConditionState::Met,
            format!("{whose} has no checks of ours"),
        ),
    };
    condition("checks", state, detail)
}

fn merge_conditions(skip_review: bool, snapshot: &ScheduleSnapshot) -> Vec<Condition> {
    let order = if snapshot.base_is_trunk {
        condition("order", ConditionState::Met, "its base is the trunk")
    } else {
        condition(
            "order",
            ConditionState::Waiting,
            "its base is an open item that has to merge first",
        )
    };
    let current = if snapshot.inbox_conflicting > 0 {
        condition(
            "current",
            ConditionState::Blocked,
            format!(
                "{} inbox commit(s) are predicted to conflict",
                snapshot.inbox_conflicting
            ),
        )
    } else if snapshot.inbox_clean > 0 {
        condition(
            "current",
            ConditionState::Waiting,
            format!("{} inbox commit(s) still to take", snapshot.inbox_clean),
        )
    } else {
        condition("current", ConditionState::Met, "the inbox is empty")
    };
    let review = match snapshot.review {
        ReviewVerdict::Approved => condition("review", ConditionState::Met, "approved"),
        ReviewVerdict::ChangesRequested => {
            condition("review", ConditionState::Blocked, "changes were requested")
        }
        ReviewVerdict::Pending if skip_review && snapshot.own_checks == ChecksVerdict::Green => {
            condition(
                "review",
                ConditionState::Met,
                "review skipped, checks are green",
            )
        }
        ReviewVerdict::Pending => condition("review", ConditionState::Waiting, "awaiting approval"),
    };
    vec![
        order,
        current,
        checks_condition(snapshot.own_checks, "its own"),
        review,
    ]
}

fn propagate_conditions(snapshot: &ScheduleSnapshot) -> Vec<Condition> {
    let taking = snapshot.inbox_clean + snapshot.inbox_conflicting;
    let inbox = if taking > 0 {
        condition(
            "inbox",
            ConditionState::Met,
            format!("{taking} commit(s) to take"),
        )
    } else {
        condition("inbox", ConditionState::Waiting, "nothing to take yet")
    };
    let conflicts = if snapshot.inbox_conflicting > 0 {
        condition(
            "conflicts",
            ConditionState::Blocked,
            format!(
                "{} commit(s) are predicted to conflict",
                snapshot.inbox_conflicting
            ),
        )
    } else {
        condition("conflicts", ConditionState::Met, "no predicted conflicts")
    };
    let worktree = match snapshot.worktree {
        WorktreeVerdict::Clean => condition(
            "worktree",
            ConditionState::Met,
            "nothing uncommitted or unpushed",
        ),
        WorktreeVerdict::Dirty => {
            condition("worktree", ConditionState::Blocked, "uncommitted changes")
        }
        WorktreeVerdict::Unpushed => {
            condition("worktree", ConditionState::Blocked, "unpushed commits")
        }
    };
    vec![
        inbox,
        conflicts,
        checks_condition(snapshot.base_checks, "the base's"),
        worktree,
    ]
}

/// Evaluate `schedule` against `snapshot`.
pub fn evaluate(schedule: &Schedule, snapshot: &ScheduleSnapshot) -> ScheduleView {
    let conditions = match &schedule.kind {
        ScheduleKind::Merge { skip_review } => merge_conditions(*skip_review, snapshot),
        ScheduleKind::Propagate => propagate_conditions(snapshot),
    };
    let status = conditions
        .iter()
        .map(|c| c.state)
        .max()
        .unwrap_or(ConditionState::Met);
    let held_up_on = [ConditionState::Blocked, ConditionState::Waiting]
        .into_iter()
        .find_map(|state| conditions.iter().find(|c| c.state == state))
        .map(|c| c.name);
    ScheduleView {
        schedule: schedule.clone(),
        conditions,
        status,
        held_up_on,
    }
}

/// Merges for every item of a wave that has an open PR, bottom-up (`items` in line order with whether
/// each has an open, unmerged PR). `armed_at` is left empty for the caller to stamp.
pub fn schedule_wave(items: &[(String, bool)], skip_review: bool, armed_by: &str) -> Vec<Schedule> {
    items
        .iter()
        .filter(|(_, has_open_pr)| *has_open_pr)
        .map(|(item_id, _)| Schedule {
            item_id: item_id.clone(),
            kind: ScheduleKind::Merge { skip_review },
            armed_at: String::new(),
            armed_by: armed_by.to_string(),
        })
        .collect()
}

/// Why arming, disarming or reading schedules was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleError {
    NoSuchItem(String),
    /// Nothing is armed on the item.
    NotArmed(String),
}

impl std::fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuchItem(id) => write!(f, "item '{id}' not found"),
            Self::NotArmed(id) => write!(f, "item '{id}' has no armed merge or propagation"),
        }
    }
}

impl std::error::Error for ScheduleError {}

/// Arm `kind` on `item_id`, replacing what was armed there. `armed_at` is the caller's clock.
pub fn arm(
    queue: &mut WorkQueue,
    item_id: &str,
    kind: ScheduleKind,
    armed_at: &str,
    armed_by: &str,
) -> Result<Schedule, ScheduleError> {
    if queue.node(item_id).is_none() {
        return Err(ScheduleError::NoSuchItem(item_id.to_string()));
    }
    let schedule = Schedule {
        item_id: item_id.to_string(),
        kind,
        armed_at: armed_at.to_string(),
        armed_by: armed_by.to_string(),
    };
    queue.schedules.retain(|s| s.item_id != item_id);
    queue.schedules.push(schedule.clone());
    Ok(schedule)
}

/// Disarm `item_id`.
pub fn disarm(queue: &mut WorkQueue, item_id: &str) -> Result<(), ScheduleError> {
    let before = queue.schedules.len();
    queue.schedules.retain(|s| s.item_id != item_id);
    if queue.schedules.len() == before {
        return Err(ScheduleError::NotArmed(item_id.to_string()));
    }
    Ok(())
}

/// Arm a merge on each item of `wave` that `has_open_pr` says is open, bottom-up (the wave's items in
/// line order). Returns the schedules armed.
pub fn arm_wave(
    queue: &mut WorkQueue,
    wave_items: &[String],
    has_open_pr: impl Fn(&str) -> bool,
    skip_review: bool,
    armed_at: &str,
    armed_by: &str,
) -> Result<Vec<Schedule>, ScheduleError> {
    if let Some(unknown) = wave_items.iter().find(|id| queue.node(id).is_none()) {
        return Err(ScheduleError::NoSuchItem(unknown.clone()));
    }
    let items: Vec<(String, bool)> = wave_items
        .iter()
        .map(|id| (id.clone(), has_open_pr(id)))
        .collect();
    let mut armed = Vec::new();
    for planned in schedule_wave(&items, skip_review, armed_by) {
        armed.push(arm(
            queue,
            &planned.item_id,
            planned.kind,
            armed_at,
            armed_by,
        )?);
    }
    Ok(armed)
}

/// One check run as the verdict reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Running,
    Passed,
    Failed,
}

/// The verdict over a head commit's check runs. No runs is `Pending`, never green: nothing reported
/// is not "all passed". A failed run outranks a running one.
pub fn checks_verdict(runs: &[RunOutcome]) -> ChecksVerdict {
    if runs.is_empty() || runs.iter().all(|r| *r == RunOutcome::Running) {
        return ChecksVerdict::Pending;
    }
    if runs.contains(&RunOutcome::Failed) {
        return ChecksVerdict::Failed;
    }
    if runs.contains(&RunOutcome::Running) {
        return ChecksVerdict::Pending;
    }
    ChecksVerdict::Green
}

/// The verdict over a PR's reviews (`(author, GitHub state)` in submission order): each author's
/// latest approving or change-requesting review counts (`COMMENTED` changes nothing, `DISMISSED`
/// withdraws). Any outstanding change request wins over approvals.
pub fn review_verdict<'a>(reviews: impl IntoIterator<Item = (&'a str, &'a str)>) -> ReviewVerdict {
    let mut latest: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for (author, state) in reviews {
        match state {
            "APPROVED" | "CHANGES_REQUESTED" => {
                latest.insert(author, state);
            }
            "DISMISSED" => {
                latest.remove(author);
            }
            _ => {}
        }
    }
    if latest.values().any(|s| *s == "CHANGES_REQUESTED") {
        ReviewVerdict::ChangesRequested
    } else if latest.values().any(|s| *s == "APPROVED") {
        ReviewVerdict::Approved
    } else {
        ReviewVerdict::Pending
    }
}
