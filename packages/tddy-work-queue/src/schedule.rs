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

/// What a schedule fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleKind {
    Merge { skip_review: bool },
    Propagate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub item_id: String,
    pub kind: ScheduleKind,
    /// ISO-8601, stamped by whoever arms it; empty until then.
    pub armed_at: String,
    pub armed_by: String,
}

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
