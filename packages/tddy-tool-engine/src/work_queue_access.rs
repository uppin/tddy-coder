//! Who may use a session's work queue (`#workqueue` 5/14). **Access follows the queue, never a role**:
//! the session that owns a queue has every work tool on it; a session one of its items links to is
//! *addressed* by it and may read it; everyone else has nothing. Nothing in this module reads a
//! session role.
//!
//! There is no creation tool — a queue is created only at session start.

use tddy_core::changeset::WorkQueue;

use crate::catalog::ToolDef;
use crate::work_item_git_tools::{is_stack_git_tool_offered_to, stack_git_tool_defs};
use crate::work_queue_tools::is_stack_tool_offered_to;

/// What a session may do with one queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkQueueAccess {
    /// The queue's own session: every work tool.
    Owner,
    /// A session an item of the queue links to: the reads.
    Addressed,
}

/// `caller`'s access to the queue `owner` owns, `None` when it has none.
pub fn work_queue_access_of(
    caller: &str,
    owner: &str,
    queue: &WorkQueue,
) -> Option<WorkQueueAccess> {
    if caller == owner {
        return Some(WorkQueueAccess::Owner);
    }
    queue
        .nodes
        .iter()
        .any(|item| item.session_id.as_deref() == Some(caller))
        .then_some(WorkQueueAccess::Addressed)
}

/// The work tools a session with `access` is offered (none for `None`): the plan and read tools,
/// then the git tools, each in advertised order.
pub fn work_queue_tools_offered_to(access: Option<WorkQueueAccess>) -> Vec<ToolDef> {
    let Some(access) = access else {
        return Vec::new();
    };
    let plan = crate::work_queue_tools::stack_tool_defs()
        .into_iter()
        .filter(|tool| is_stack_tool_offered_to(access, &tool.name));
    let git = stack_git_tool_defs()
        .into_iter()
        .filter(|tool| is_stack_git_tool_offered_to(access, &tool.name));
    plan.chain(git).collect()
}
