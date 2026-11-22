//! A stack tool's caller, classified from the records on this host (`#orchestrators` 13/13).
//!
//! The classification [`work_queue_caller_role`](super::work_queue_caller_role) decides from facts; this
//! module reads those facts — a session's `.session.yaml` role, its changeset's `queue_id` and
//! `orchestrator_session_id`, and the project's stack store — from a sessions base
//! (`<base>/sessions/<id>`, `<base>/projects`). It lives here, below both of its callers, so the
//! daemon's `ListExecTools` (`tddy-daemon-rpc`) and the jail bridge's
//! (`tddy-session-lifecycle`, which `tddy-daemon-rpc` depends on) classify a session the same way
//! and offer it the same stack tools.
//!
//! A call forwarded from another host is classified here too
//! ([`forwarded_stack_caller_role`]): the caller's owner reports its role, and this host — which
//! holds the stack — checks its own store for a node bound to the caller
//! ([`stack_bound_to_session_here`]).
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-14-stack-node-reads.md.

use std::path::Path;

use tddy_core::changeset::Changeset;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::is_orchestrator_role;
use tddy_core::work_queue_store::{WorkQueueStore, WorkQueueStoreError};
use tddy_core::WorkflowError;

use super::{
    work_queue_caller_role, work_queue_tools_offered_to, ParentStack, WorkQueueCaller,
    WorkQueueCallerFacts, WorkQueueCallerRole,
};
use crate::catalog::ToolDef;
use crate::work_item_git_tools::work_item_git_tools_offered_to;

/// A session whose records cannot be read, so whose stack role cannot be known. The daemon refuses
/// such a caller rather than treating it as ordinary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkQueueCallerUnreadable {
    pub session_id: String,
    /// Which record: `"record"`, `"changeset"`, `"orchestrator's stack"`, `"stack store"`.
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

/// The stack role of `session_id`, read from its records under `sessions_base`: its `.session.yaml`
/// role, its changeset's `queue_id` and `orchestrator_session_id`, and — for a session that names
/// an orchestrator — that orchestrator's stack in the project's store. `Ok(None)` for a session
/// offered no stack tool; `Err` for a session whose records cannot be read.
pub fn work_queue_caller_from_records(
    sessions_base: &Path,
    session_id: &str,
) -> Result<Option<WorkQueueCaller>, WorkQueueCallerUnreadable> {
    let unreadable = |what: &'static str, why: String| WorkQueueCallerUnreadable {
        session_id: session_id.to_string(),
        what,
        why,
    };
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    let metadata = tddy_core::read_session_metadata(&session_dir)
        .map_err(|e| unreadable("record", e.to_string()))?;
    let changeset =
        changeset_of(&session_dir).map_err(|e| unreadable("changeset", e.to_string()))?;
    let parent_stack = match changeset.orchestrator_session_id.as_deref() {
        Some(orchestrator) => node_binding(
            sessions_base,
            &metadata.project_id,
            orchestrator,
            session_id,
        )
        .map_err(|e| unreadable("orchestrator's stack", e))?,
        None => None,
    };
    let facts = WorkQueueCallerFacts {
        session_role: metadata.session_role.as_deref(),
        own_stack_id: changeset.work_queue_id.as_deref(),
        parent_stack: parent_stack
            .as_ref()
            .map(|(queue_id, binds_this_session)| ParentStack {
                queue_id,
                binds_this_session: *binds_this_session,
            }),
    };
    Ok(work_queue_caller_role(&facts).map(|role| WorkQueueCaller {
        session_id: session_id.to_string(),
        project_id: metadata.project_id.clone(),
        role,
    }))
}

/// The session's changeset; a session that has none yet (an ordinary one) has the default.
fn changeset_of(session_dir: &Path) -> Result<Changeset, WorkflowError> {
    match tddy_core::read_changeset(session_dir) {
        Err(WorkflowError::ChangesetMissing(_)) => Ok(Default::default()),
        read => read,
    }
}

/// The stack `orchestrator`'s changeset names and whether one of its nodes is bound to
/// `session_id`; `None` when that orchestrator is not on this host, names no stack, or names one the
/// project no longer holds.
fn node_binding(
    sessions_base: &Path,
    project_id: &str,
    orchestrator: &str,
    session_id: &str,
) -> Result<Option<(String, bool)>, String> {
    let orchestrator_dir = unified_session_dir_path(sessions_base, orchestrator);
    let changeset = match tddy_core::read_changeset(&orchestrator_dir) {
        Ok(changeset) => changeset,
        Err(WorkflowError::ChangesetMissing(_)) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let Some(queue_id) = changeset.work_queue_id else {
        return Ok(None);
    };
    let store = WorkQueueStore::new(sessions_base.join("projects"));
    let record = match store.read(project_id, &queue_id) {
        Ok(record) => record,
        Err(WorkQueueStoreError::NotFound { .. }) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let binds_this_session = record
        .stack
        .nodes
        .iter()
        .any(|node| node.session_id.as_deref() == Some(session_id));
    Ok(Some((queue_id, binds_this_session)))
}

/// The stack of `project_id` held under `sessions_base` that has a node bound to `session_id`
/// (`WorkItem.session_id`), if any. Read by the host that holds the stack for a caller another
/// host holds.
pub fn stack_bound_to_session_here(
    sessions_base: &Path,
    project_id: &str,
    session_id: &str,
) -> Result<Option<String>, WorkQueueCallerUnreadable> {
    let store = WorkQueueStore::new(sessions_base.join("projects"));
    let records = store
        .list(project_id)
        .map_err(|e| WorkQueueCallerUnreadable {
            session_id: session_id.to_string(),
            what: "stack store",
            why: e.to_string(),
        })?;
    Ok(records.into_iter().find_map(|record| {
        record
            .stack
            .nodes
            .iter()
            .any(|node| node.session_id.as_deref() == Some(session_id))
            .then_some(record.queue_id)
    }))
}

/// The stack role of a caller another host holds, given the role its owner reports for it
/// (`ListSessions`' `session_role`) and the stack this host holds with a node bound to it. An
/// orchestrator role wins and owns no stack here; otherwise a bound node reads its own stack.
pub fn forwarded_stack_caller_role(
    listed_session_role: &str,
    bound_stack_id: Option<&str>,
) -> Option<WorkQueueCallerRole> {
    if is_orchestrator_role(listed_session_role) {
        return Some(WorkQueueCallerRole::Orchestrator { own_stack_id: None });
    }
    bound_stack_id.map(|queue_id| WorkQueueCallerRole::WorkItem {
        queue_id: queue_id.to_string(),
    })
}

/// Every stack tool and stack git tool `role` is offered, stack tools first — what a session's
/// `ListExecTools` appends to its catalogue on the daemon and in its jail alike.
pub fn stack_and_git_tools_offered_to(role: Option<&WorkQueueCallerRole>) -> Vec<ToolDef> {
    let mut tools = work_queue_tools_offered_to(role);
    tools.extend(work_item_git_tools_offered_to(role));
    tools
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_changeset::work_queue_fixtures::{a_stack_host, queue_id_of, AStackHost, A_PROJECT};
    use tddy_core::changeset::{Changeset, WorkItem, WorkQueue};

    use super::*;
    use crate::work_item_git_tools::WORK_ITEM_GIT_TOOL_NAMES;
    use crate::work_queue_tools::WORK_QUEUE_TOOL_NAMES;

    fn a_stack_binding(child: Option<&str>) -> WorkQueue {
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

    fn with_role(dir: &Path, role: &str) {
        let mut meta = tddy_core::read_session_metadata(dir).unwrap();
        meta.session_role = Some(role.to_string());
        tddy_core::session_metadata::write_session_metadata(dir, &meta).unwrap();
    }

    /// An orchestrator-role session `orch` whose stack binds `child` to node n1; the stack's id.
    fn an_orchestrator_binding(host: &AStackHost, child: Option<&str>) -> String {
        let orch = host.orchestrator("orch", a_stack_binding(child));
        with_role(&orch, "orchestrator");
        queue_id_of(&orch).expect("the orchestrator's changeset names its stack")
    }

    fn a_child_of_orch(host: &AStackHost, child: &str) {
        host.session(
            child,
            Changeset {
                orchestrator_session_id: Some("orch".to_string()),
                ..Changeset::default()
            },
        );
    }

    fn names(tools: Vec<ToolDef>) -> Vec<String> {
        tools.into_iter().map(|tool| tool.name).collect()
    }

    // ── work_queue_caller_from_records ────────────────────────────────────────────────────────────

    #[test]
    fn a_session_its_orchestrators_stack_binds_is_classified_a_node_of_that_stack() {
        // Given an orchestrator whose node n1 is bound to a child session naming it
        let host = a_stack_host();
        let queue_id = an_orchestrator_binding(&host, Some("child"));
        a_child_of_orch(&host, "child");

        // When the child is classified from the host's records
        let caller = work_queue_caller_from_records(host.base(), "child").unwrap();

        // Then it is a node of that stack, in its project
        assert_eq!(
            caller,
            Some(WorkQueueCaller {
                session_id: "child".to_string(),
                project_id: A_PROJECT.to_string(),
                role: WorkQueueCallerRole::WorkItem { queue_id },
            })
        );
    }

    #[test]
    fn an_orchestrator_role_session_is_classified_owning_the_stack_its_changeset_names() {
        // Given an orchestrator-role session whose stack the project's store holds
        let host = a_stack_host();
        let own = an_orchestrator_binding(&host, None);

        // When it is classified
        let caller = work_queue_caller_from_records(host.base(), "orch").unwrap();

        // Then it is an orchestrator owning that stack
        assert_eq!(
            caller.map(|c| c.role),
            Some(WorkQueueCallerRole::Orchestrator {
                own_stack_id: Some(own)
            })
        );
    }

    #[test]
    fn a_child_its_orchestrators_stack_does_not_bind_is_classified_with_no_stack_role() {
        // Given a session naming an orchestrator whose stack binds no node to it
        let host = a_stack_host();
        an_orchestrator_binding(&host, Some("someone-else"));
        a_child_of_orch(&host, "child");

        // When it is classified
        let caller = work_queue_caller_from_records(host.base(), "child").unwrap();

        // Then it has no stack role
        assert_eq!(caller, None);
    }

    #[test]
    fn a_session_with_no_record_cannot_be_classified() {
        // Given a session id with nothing on disk
        let host = a_stack_host();

        // When it is classified
        let unreadable = work_queue_caller_from_records(host.base(), "ghost")
            .expect_err("its role cannot be known");

        // Then the refusal names the session and its missing record
        assert_eq!(
            (unreadable.session_id.as_str(), unreadable.what),
            ("ghost", "record")
        );
    }

    // ── stack_bound_to_session_here ──────────────────────────────────────────────────────────

    #[test]
    fn the_stack_with_a_node_bound_to_the_session_is_found_on_the_host_that_holds_it() {
        // Given a stack of the project whose node n1 is bound to a session held on another host
        let host = a_stack_host();
        let queue_id = an_orchestrator_binding(&host, Some("held-elsewhere"));

        // When this host looks for a stack binding that session
        let bound = stack_bound_to_session_here(host.base(), A_PROJECT, "held-elsewhere").unwrap();

        // Then it finds that stack
        assert_eq!(bound, Some(queue_id));
    }

    #[test]
    fn a_session_no_stack_of_the_project_binds_is_bound_to_none() {
        // Given a stack of the project binding another session
        let host = a_stack_host();
        an_orchestrator_binding(&host, Some("someone-else"));

        // When this host looks for a stack binding a different session
        let bound = stack_bound_to_session_here(host.base(), A_PROJECT, "held-elsewhere").unwrap();

        // Then none binds it
        assert_eq!(bound, None);
    }

    // ── forwarded_stack_caller_role ──────────────────────────────────────────────────────────

    #[test]
    fn a_forwarded_caller_its_owner_lists_as_an_orchestrator_owns_no_stack_here() {
        // Given a caller its owner lists with the orchestrator role, bound to no node here

        // When its stack role is decided
        let role = forwarded_stack_caller_role("orchestrator", None);

        // Then it is an orchestrator that must name the stack it acts on
        assert_eq!(
            role,
            Some(WorkQueueCallerRole::Orchestrator { own_stack_id: None })
        );
    }

    #[test]
    fn a_forwarded_caller_bound_to_a_node_here_is_a_node_of_that_stack() {
        // Given a caller its owner lists with no role, bound to a node of a stack held here

        // When its stack role is decided
        let role = forwarded_stack_caller_role("", Some("stack-1"));

        // Then it is a node of that stack
        assert_eq!(
            role,
            Some(WorkQueueCallerRole::WorkItem {
                queue_id: "stack-1".to_string()
            })
        );
    }

    #[test]
    fn an_orchestrator_role_wins_over_a_node_binding_for_a_forwarded_caller() {
        // Given a caller listed as main (project-home) and also bound to a node held here

        // When its stack role is decided
        let role = forwarded_stack_caller_role("project-home", Some("stack-1"));

        // Then it is an orchestrator, as on its own host
        assert_eq!(
            role,
            Some(WorkQueueCallerRole::Orchestrator { own_stack_id: None })
        );
    }

    #[test]
    fn a_forwarded_caller_with_no_role_and_no_binding_has_no_stack_role() {
        // Given a caller listed with no role and bound to nothing here

        // When its stack role is decided
        let role = forwarded_stack_caller_role("", None);

        // Then it has none
        assert_eq!(role, None);
    }

    // ── stack_and_git_tools_offered_to ───────────────────────────────────────────────────────

    #[test]
    fn a_node_is_offered_status_and_read_and_no_git_tool() {
        // Given a node of a stack
        let node = WorkQueueCallerRole::WorkItem {
            queue_id: "stack-1".to_string(),
        };

        // When the stack tools it is offered are listed
        let offered = names(stack_and_git_tools_offered_to(Some(&node)));

        // Then they are the two reads alone
        assert_eq!(offered, vec!["work_queue_status", "work_pr_read"]);
    }

    #[test]
    fn an_orchestrator_is_offered_every_stack_tool_then_every_stack_git_tool() {
        // Given an orchestrator
        let orchestrator = WorkQueueCallerRole::Orchestrator { own_stack_id: None };

        // When the stack tools it is offered are listed
        let offered = names(stack_and_git_tools_offered_to(Some(&orchestrator)));

        // Then they are the eleven stack tools followed by the six git tools
        let every: Vec<String> = WORK_QUEUE_TOOL_NAMES
            .iter()
            .chain(WORK_ITEM_GIT_TOOL_NAMES.iter())
            .map(|name| name.to_string())
            .collect();
        assert_eq!(offered, every);
    }
}
