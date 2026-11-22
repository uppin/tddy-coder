//! Who may use a session's work queue (`#workqueue` 5/14). **Access follows the queue, never a role**:
//! the session that owns a queue has every work tool on it; a session one of its items links to is
//! *addressed* by it and may read it; everyone else has nothing. Nothing in this module reads a
//! session role.
//!
//! The tool names are today's constants (renamed by `#workqueue` 3/14); the creation tool is no longer
//! offered — a queue is created only at session start.

use tddy_core::changeset::Stack;

use crate::catalog::ToolDef;

/// What a session may do with one queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkQueueAccess {
    /// The queue's own session: every work tool.
    Owner,
    /// A session an item of the queue links to: the reads.
    Addressed,
}

/// `caller`'s access to the queue `owner` owns, `None` when it has none.
pub fn work_queue_access_of(caller: &str, owner: &str, queue: &Stack) -> Option<WorkQueueAccess> {
    // TODO(n5 session-work-queue): implement
    let _ = (caller, owner, queue);
    todo!("TODO(n5): implement work_queue_access_of")
}

/// The work tools a session with `access` is offered (none for `None`).
pub fn work_queue_tools_offered_to(access: Option<WorkQueueAccess>) -> Vec<ToolDef> {
    // TODO(n5 session-work-queue): implement
    let _ = access;
    todo!("TODO(n5): implement work_queue_tools_offered_to")
}
