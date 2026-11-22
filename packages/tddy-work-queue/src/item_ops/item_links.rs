//! A work item's links (`#workqueue` 6/14): an existing pull request assigned to it, and the tddy
//! sessions linked to it (any number, any host of the project). Pure operations on a queue; the tools,
//! RPCs and the UI call them under the queue's record lock.
//!
//! An assigned PR's head branch becomes the item's branch; a different branch already recorded on the
//! item is refused, never overwritten. A linked session is addressed by the queue (it may read it).
//!
//! The queue and item types are today's `WorkQueue`/`WorkItem`; the link
//! data lives on the item from `/green` on — the accessors below are how callers read it.

use tddy_core::changeset::WorkQueue;

/// The pull request assigned to an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignedPr {
    pub number: u64,
    pub url: String,
    pub head_branch: String,
}

/// A tddy session linked to an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedSession {
    pub session_id: String,
    /// The daemon holding the session; empty for the queue's own host.
    pub daemon_instance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemLinkError {
    NoSuchItem(String),
    /// The PR's head branch is not the branch the item already records.
    BranchMismatch {
        item_branch: String,
        pr_branch: String,
    },
    /// The session is not linked to the item.
    NotLinked {
        session_id: String,
    },
}

/// Assign `pr` to `item_id`, recording its head branch as the item's branch.
pub fn assign_pr(queue: &mut WorkQueue, item_id: &str, pr: AssignedPr) -> Result<(), ItemLinkError> {
    // TODO(n6 pr-and-session-link): implement
    let _ = (queue, item_id, pr);
    todo!("TODO(n6): implement assign_pr")
}

/// Remove `item_id`'s assigned PR (its branch stays).
pub fn unassign_pr(queue: &mut WorkQueue, item_id: &str) -> Result<(), ItemLinkError> {
    // TODO(n6 pr-and-session-link): implement
    let _ = (queue, item_id);
    todo!("TODO(n6): implement unassign_pr")
}

/// Link `session` to `item_id`; linking it again changes nothing.
pub fn link_session(
    queue: &mut WorkQueue,
    item_id: &str,
    session: LinkedSession,
) -> Result<(), ItemLinkError> {
    // TODO(n6 pr-and-session-link): implement
    let _ = (queue, item_id, session);
    todo!("TODO(n6): implement link_session")
}

/// Unlink `session_id` from `item_id`; refused when it is not linked.
pub fn unlink_session(
    queue: &mut WorkQueue,
    item_id: &str,
    session_id: &str,
) -> Result<(), ItemLinkError> {
    // TODO(n6 pr-and-session-link): implement
    let _ = (queue, item_id, session_id);
    todo!("TODO(n6): implement unlink_session")
}

/// `item_id`'s assigned PR, if any.
pub fn assigned_pr_of(queue: &WorkQueue, item_id: &str) -> Option<AssignedPr> {
    // TODO(n6 pr-and-session-link): implement
    let _ = (queue, item_id);
    todo!("TODO(n6): implement assigned_pr_of")
}

/// The sessions linked to `item_id`, in link order.
pub fn linked_sessions_of(queue: &WorkQueue, item_id: &str) -> Vec<LinkedSession> {
    // TODO(n6 pr-and-session-link): implement
    let _ = (queue, item_id);
    todo!("TODO(n6): implement linked_sessions_of")
}
