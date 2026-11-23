//! `ScheduleWorkItem`, `ScheduleWorkQueueWave`, `UnscheduleWorkItem` and `ListWorkQueueSchedules`
//! (`#workqueue` 13/14): arm a merge or a propagation on an item, arm a wave's merges bottom-up,
//! disarm, and read every armed schedule with its conditions evaluated live.
//!
//! The facts a schedule is judged on are gathered here, each from the surface that owns it: the line
//! parent (`lineage`), the inbox and its conflict predictions (`commit_flow`), checks and reviews (the
//! GitHub inbox, rows other readers share), and the worktree's sync state. A fact that cannot be read
//! never reads as met: the schedule is shown `waiting` on `facts`, with the reason. Nothing here fires
//! a schedule (`#workqueue` 14/14).

use std::path::Path;

use tddy_rpc::Status;
use tddy_work_queue::schedule::ScheduleError;
use tddy_worktree_service::pull_request_view::SessionPullRequests;

use super::project_stacks::store_status;
use tddy_core::work_queue_store::WorkQueueStore;

mod evaluate;
mod facts;
mod rpc;

use facts::*;
pub(crate) use facts::{status_view, unreadable_views};
pub(crate) use rpc::json;

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

pub(super) fn store_failure(
    error: tddy_core::work_queue_store::WorkQueueStoreError,
) -> ScheduleOpError {
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
