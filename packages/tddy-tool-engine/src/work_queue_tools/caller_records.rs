//! A work tool's caller and its access, read from the records on this host (`#workqueue` 5/14).
//!
//! Access follows the queue, never a role: the session that owns a queue (its id is the queue's id)
//! has [`WorkQueueAccess::Owner`]; a session an item of a project's queue links to has
//! [`WorkQueueAccess::Addressed`]. This module reads those facts — a session's `.session.yaml` for its
//! project, and the queues of the sessions under a sessions base (`<base>/sessions/<id>`) — so the
//! daemon's `ListExecTools` (`tddy-daemon-rpc`) and the jail bridge's (`tddy-session-lifecycle`)
//! offer a session the same tools, and the daemon serving a call checks the same access.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-session-work-queue.md.

use std::path::Path;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_work_queue::{SessionWorkQueueError, SessionWorkQueues};
use tddy_core::work_queue_store::{WorkQueueStore, WorkQueueStoreError};

use super::WorkQueueCaller;
use crate::catalog::ToolDef;
use crate::work_queue_access::{work_queue_access_of, WorkQueueAccess};

/// A session whose records cannot be read, so whose access cannot be known. The daemon refuses such
/// a caller rather than treating it as ordinary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkQueueCallerUnreadable {
    pub session_id: String,
    /// Which record: `"record"`, `"work queue"`.
    pub what: &'static str,
    pub why: String,
}

impl std::fmt::Display for WorkQueueCallerUnreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "session {}'s {} cannot be read ({})",
            self.session_id, self.what, self.why
        )
    }
}

impl std::error::Error for WorkQueueCallerUnreadable {}

/// The caller `session_id` is on this host — its project, from its `.session.yaml` — and the access
/// its records give it: `Owner` when it owns a queue, else `Addressed` when an item of a queue of its
/// project links it, else `None` (offered no work tool). `Err` for a session whose records cannot be
/// read.
pub fn work_queue_caller_from_records(
    sessions_base: &Path,
    session_id: &str,
) -> Result<Option<(WorkQueueCaller, WorkQueueAccess)>, WorkQueueCallerUnreadable> {
    let unreadable = |what: &'static str, why: String| WorkQueueCallerUnreadable {
        session_id: session_id.to_string(),
        what,
        why,
    };
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    let metadata = tddy_core::read_session_metadata(&session_dir)
        .map_err(|e| unreadable("record", e.to_string()))?;
    let caller = WorkQueueCaller {
        session_id: session_id.to_string(),
        project_id: metadata.project_id,
    };
    let store = WorkQueueStore::new(sessions_base);
    let records = store
        .list(&caller.project_id)
        .map_err(|e| unreadable("work queue", e.to_string()))?;
    let access = records
        .iter()
        .filter_map(|record| {
            work_queue_access_of(session_id, record.owner_session_id(), &record.stack)
        })
        .max_by_key(|access| *access == WorkQueueAccess::Owner);
    Ok(access.map(|access| (caller, access)))
}

/// What `caller` may do with the queue `queue_id` (a session id) held under `sessions_base`: the
/// access its owner or its items give it, or the refusal naming why there is none — the session owns no
/// queue (with the hint naming how one comes to exist), or the queue is not the caller's.
pub fn access_to_queue_here(
    sessions_base: &Path,
    caller: &WorkQueueCaller,
    queue_id: &str,
) -> Result<WorkQueueAccess, String> {
    let store = WorkQueueStore::new(sessions_base);
    let record = match store.read(&caller.project_id, queue_id) {
        Ok(record) => record,
        Err(WorkQueueStoreError::NotFound { .. }) => {
            return Err(
                match SessionWorkQueues::new(sessions_base.join("sessions")).read(queue_id) {
                    Ok(Some(_)) => not_the_callers(queue_id),
                    Ok(None) => SessionWorkQueueError::NoQueue {
                        session_id: queue_id.to_string(),
                    }
                    .to_string(),
                    Err(e) => e.to_string(),
                },
            )
        }
        Err(e) => return Err(e.to_string()),
    };
    work_queue_access_of(&caller.session_id, record.owner_session_id(), &record.stack)
        .ok_or_else(|| not_the_callers(queue_id))
}

fn not_the_callers(queue_id: &str) -> String {
    format!(
        "work queue {queue_id} is not this session's: only the session that owns it and the \
         sessions its items link to may use it"
    )
}

/// Every work tool and work git tool a session with `access` is offered, plan tools first — what a
/// session's `ListExecTools` appends to its catalogue on the daemon and in its jail alike.
pub fn stack_and_git_tools_offered_to(access: Option<WorkQueueAccess>) -> Vec<ToolDef> {
    crate::work_queue_access::work_queue_tools_offered_to(access)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_changeset::work_queue_fixtures::{a_stack_host, AStackHost, A_PROJECT};
    use tddy_core::changeset::{Changeset, WorkItem, WorkQueue};

    use super::*;
    use crate::work_item_git_tools::WORK_ITEM_GIT_TOOL_NAMES;
    use crate::work_queue_tools::WORK_QUEUE_TOOL_NAMES;

    fn a_queue_linking(child: Option<&str>) -> WorkQueue {
        WorkQueue {
            version: 1,
            nodes: vec![WorkItem {
                node_id: "n1".to_string(),
                title: "Store".to_string(),
                session_id: child.map(str::to_string),
                ..WorkItem::default()
            }],
        }
    }

    /// A session `owner` owning a queue that links `child` to item n1.
    fn an_owner_linking(host: &AStackHost, child: Option<&str>) {
        host.orchestrator("owner", a_queue_linking(child));
    }

    fn names(tools: Vec<ToolDef>) -> Vec<String> {
        tools.into_iter().map(|tool| tool.name).collect()
    }

    #[test]
    fn a_session_owning_a_queue_is_its_owner() {
        // Given a session that owns a queue
        let host = a_stack_host();
        an_owner_linking(&host, None);

        // When it is read from the host's records
        let caller = work_queue_caller_from_records(host.base(), "owner").unwrap();

        // Then it is the owner, in its project
        assert_eq!(
            caller,
            Some((
                WorkQueueCaller {
                    session_id: "owner".to_string(),
                    project_id: A_PROJECT.to_string(),
                },
                WorkQueueAccess::Owner
            ))
        );
    }

    #[test]
    fn a_session_an_item_links_to_is_addressed() {
        // Given a queue linking a child
        let host = a_stack_host();
        an_owner_linking(&host, Some("child"));
        host.session("child", Changeset::default());

        // When the child is read
        let caller = work_queue_caller_from_records(host.base(), "child").unwrap();

        // Then it is addressed
        assert_eq!(
            caller.map(|(_, access)| access),
            Some(WorkQueueAccess::Addressed)
        );
    }

    #[test]
    fn a_session_no_queue_links_and_owning_none_has_no_access() {
        // Given a queue linking someone else
        let host = a_stack_host();
        an_owner_linking(&host, Some("someone-else"));
        host.session("child", Changeset::default());

        // When the child is read
        let caller = work_queue_caller_from_records(host.base(), "child").unwrap();

        // Then it has none
        assert_eq!(caller, None);
    }

    #[test]
    fn a_session_with_no_record_cannot_be_read() {
        // Given a session id with nothing on disk
        let host = a_stack_host();

        // When it is read
        let unreadable = work_queue_caller_from_records(host.base(), "ghost")
            .expect_err("its access cannot be known");

        // Then the refusal names the session and its missing record
        assert_eq!(
            (unreadable.session_id.as_str(), unreadable.what),
            ("ghost", "record")
        );
    }

    #[test]
    fn addressing_a_session_that_owns_no_queue_is_refused_with_the_hint() {
        // Given a caller and another session owning no queue
        let host = a_stack_host();
        host.session("caller", Changeset::default());
        host.session("plain", Changeset::default());
        let caller = WorkQueueCaller {
            session_id: "caller".to_string(),
            project_id: A_PROJECT.to_string(),
        };

        // When the caller addresses it
        let refusal = access_to_queue_here(host.base(), &caller, "plain").unwrap_err();

        // Then the refusal names the session and how a queue comes to exist
        assert_eq!(
            refusal,
            format!(
                "session plain has no work queue — {}",
                tddy_core::session_work_queue::NO_WORK_QUEUE_HINT
            )
        );
    }

    #[test]
    fn addressing_a_queue_that_is_not_the_callers_is_refused() {
        // Given another session's queue that links nobody
        let host = a_stack_host();
        an_owner_linking(&host, None);
        let caller = WorkQueueCaller {
            session_id: "main".to_string(),
            project_id: A_PROJECT.to_string(),
        };

        // When a session unrelated to it addresses it
        let refusal = access_to_queue_here(host.base(), &caller, "owner").unwrap_err();

        // Then it is refused as not its own
        assert!(refusal.contains("is not this session's"), "{refusal}");
    }

    #[test]
    fn the_owner_is_offered_every_tool_then_every_git_tool_and_an_addressed_session_the_reads() {
        // When the offered tools are listed for each access
        let owner = names(stack_and_git_tools_offered_to(Some(WorkQueueAccess::Owner)));
        let addressed = names(stack_and_git_tools_offered_to(Some(
            WorkQueueAccess::Addressed,
        )));

        // Then the owner has the ten plan tools then the six git tools, an addressed session two reads
        let every: Vec<String> = WORK_QUEUE_TOOL_NAMES
            .iter()
            .chain(WORK_ITEM_GIT_TOOL_NAMES.iter())
            .map(|name| name.to_string())
            .collect();
        assert_eq!(owner, every);
        assert_eq!(addressed, vec!["work_queue_status", "work_pr_read"]);
    }
}
