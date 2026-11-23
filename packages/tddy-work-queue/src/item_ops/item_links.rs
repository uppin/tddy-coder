//! A work item's links (`#workqueue` 6/14): an existing pull request assigned to it, and the tddy
//! sessions linked to it (any number, any host of the project). Pure operations on a queue; the tools,
//! RPCs and the UI call them under the queue's record lock.
//!
//! An assigned PR's head branch becomes the item's branch; a different branch already recorded on the
//! item is refused, never overwritten. A linked session is addressed by the queue (it may read it).
//!
//! The link data lives on the item (`WorkItem::pr`, `WorkItem::linked_sessions`); the accessors below
//! are how callers read it.

use tddy_core::changeset::{WorkItem, WorkQueue};

// The link records live on the item, so they are the changeset's types; re-exported as this module's
// published surface.
pub use tddy_core::changeset::{AssignedPr, LinkedSession};

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

impl std::fmt::Display for ItemLinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuchItem(item) => write!(f, "item '{item}' not found"),
            Self::BranchMismatch {
                item_branch,
                pr_branch,
            } => write!(
                f,
                "the pull request is on branch '{pr_branch}', but the item records branch \
                 '{item_branch}' — assign a PR on the item's own branch"
            ),
            Self::NotLinked { session_id } => {
                write!(f, "session {session_id} is not linked to the item")
            }
        }
    }
}

impl std::error::Error for ItemLinkError {}

fn item_mut<'q>(
    queue: &'q mut WorkQueue,
    item_id: &str,
) -> Result<&'q mut WorkItem, ItemLinkError> {
    queue
        .nodes
        .iter_mut()
        .find(|item| item.node_id == item_id)
        .ok_or_else(|| ItemLinkError::NoSuchItem(item_id.to_string()))
}

/// Assign `pr` to `item_id`, recording its head branch as the item's branch.
pub fn assign_pr(
    queue: &mut WorkQueue,
    item_id: &str,
    pr: AssignedPr,
) -> Result<(), ItemLinkError> {
    let item = item_mut(queue, item_id)?;
    if let Some(item_branch) = item.branch.as_deref().filter(|b| *b != pr.head_branch) {
        return Err(ItemLinkError::BranchMismatch {
            item_branch: item_branch.to_string(),
            pr_branch: pr.head_branch,
        });
    }
    item.branch = Some(pr.head_branch.clone());
    item.pr = Some(pr);
    Ok(())
}

/// Remove `item_id`'s assigned PR (its branch stays).
pub fn unassign_pr(queue: &mut WorkQueue, item_id: &str) -> Result<(), ItemLinkError> {
    item_mut(queue, item_id)?.pr = None;
    Ok(())
}

/// Link `session` to `item_id`; linking it again changes nothing.
pub fn link_session(
    queue: &mut WorkQueue,
    item_id: &str,
    session: LinkedSession,
) -> Result<(), ItemLinkError> {
    let item = item_mut(queue, item_id)?;
    if !item
        .linked_sessions
        .iter()
        .any(|linked| linked.session_id == session.session_id)
    {
        item.linked_sessions.push(session);
    }
    Ok(())
}

/// Unlink `session_id` from `item_id`; refused when it is not linked.
pub fn unlink_session(
    queue: &mut WorkQueue,
    item_id: &str,
    session_id: &str,
) -> Result<(), ItemLinkError> {
    let item = item_mut(queue, item_id)?;
    let before = item.linked_sessions.len();
    item.linked_sessions
        .retain(|linked| linked.session_id != session_id);
    if item.linked_sessions.len() == before {
        return Err(ItemLinkError::NotLinked {
            session_id: session_id.to_string(),
        });
    }
    Ok(())
}

/// `item_id`'s assigned PR, if any.
pub fn assigned_pr_of(queue: &WorkQueue, item_id: &str) -> Option<AssignedPr> {
    queue.node(item_id)?.pr.clone()
}

/// The sessions linked to `item_id`, in link order.
pub fn linked_sessions_of(queue: &WorkQueue, item_id: &str) -> Vec<LinkedSession> {
    queue
        .node(item_id)
        .map(|item| item.linked_sessions.clone())
        .unwrap_or_default()
}
