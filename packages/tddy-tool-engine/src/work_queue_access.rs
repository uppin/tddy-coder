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

/// Whether a work-tool call (`args_json`) names a queue (`queue_id`) held by **another host**
/// (`daemon_instance_id` set and not `local_daemon_instance_id`). Such a call is forwarded to that
/// host without the caller's own host checking its access: access is decided where the queue is held,
/// against that queue, and the caller's own host holds no record of it. A call that names no queue —
/// a listing, a search, a read by pull number — is checked by the caller's own host, because the host
/// it is forwarded to does not check it again.
pub fn names_a_queue_on_another_host(args_json: &str, local_daemon_instance_id: &str) -> bool {
    let Ok(args) = serde_json::from_str::<serde_json::Value>(args_json) else {
        return false;
    };
    let named = |key: &str| {
        args.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    named("queue_id").is_some()
        && named("daemon_instance_id").is_some_and(|host| host != local_daemon_instance_id)
}

#[cfg(test)]
mod tests {
    use super::names_a_queue_on_another_host;

    #[test]
    fn a_call_naming_a_queue_on_another_host_is_left_to_that_host() {
        assert!(names_a_queue_on_another_host(
            r#"{"queue_id":"owner","daemon_instance_id":"host-b"}"#,
            "host-a"
        ));
    }

    #[test]
    fn a_call_naming_no_queue_or_this_host_is_checked_here() {
        for args in [
            r#"{"daemon_instance_id":"host-b"}"#,
            r#"{"queue_id":" ","daemon_instance_id":"host-b"}"#,
            r#"{"queue_id":"owner","daemon_instance_id":"host-a"}"#,
            r#"{"queue_id":"owner"}"#,
            "",
        ] {
            assert!(!names_a_queue_on_another_host(args, "host-a"), "{args}");
        }
    }
}
