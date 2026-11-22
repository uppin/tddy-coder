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

/// Evaluate `schedule` against `snapshot`.
pub fn evaluate(schedule: &Schedule, snapshot: &ScheduleSnapshot) -> ScheduleView {
    // TODO(n13 schedules): implement
    let _ = (schedule, snapshot);
    todo!("TODO(n13): implement evaluate")
}

/// Merges for every item of a wave that has an open PR, bottom-up (`items` in line order with whether
/// each has an open, unmerged PR). `armed_at` is left empty for the caller to stamp.
pub fn schedule_wave(items: &[(String, bool)], skip_review: bool, armed_by: &str) -> Vec<Schedule> {
    // TODO(n13 schedules): implement
    let _ = (items, skip_review, armed_by);
    todo!("TODO(n13): implement schedule_wave")
}
