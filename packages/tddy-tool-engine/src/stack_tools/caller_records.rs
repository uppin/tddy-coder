//! A stack tool's caller, classified from the records on this host (`#orchestrators` 13/13).
//!
//! The classification [`stack_caller_role`](super::stack_caller_role) decides from facts; this
//! module reads those facts — a session's `.session.yaml` role, its changeset's `stack_id` and
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

use super::{StackCaller, StackCallerRole};
use crate::catalog::ToolDef;

/// A session whose records cannot be read, so whose stack role cannot be known. The daemon refuses
/// such a caller rather than treating it as ordinary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackCallerUnreadable {
    pub session_id: String,
    /// Which record: `"record"`, `"changeset"`, `"orchestrator's stack"`, `"stack store"`.
    pub what: &'static str,
    pub why: String,
}

impl std::fmt::Display for StackCallerUnreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "session {}'s {} cannot be read ({})",
            self.session_id, self.what, self.why
        )
    }
}

impl std::error::Error for StackCallerUnreadable {}

/// The stack role of `session_id`, read from its records under `sessions_base`: its `.session.yaml`
/// role, its changeset's `stack_id` and `orchestrator_session_id`, and — for a session that names
/// an orchestrator — that orchestrator's stack in the project's store. `Ok(None)` for a session
/// offered no stack tool; `Err` for a session whose records cannot be read.
pub fn stack_caller_from_records(
    sessions_base: &Path,
    session_id: &str,
) -> Result<Option<StackCaller>, StackCallerUnreadable> {
    // TODO(stack-node-reads): implement — move `stack_caller_of`'s record reads
    // (`tddy-daemon-rpc/src/exec_tool/stack_tools.rs`) here.
    let _ = (sessions_base, session_id);
    Ok(None)
}

/// The stack of `project_id` held under `sessions_base` that has a node bound to `session_id`
/// (`StackNode.session_id`), if any. Read by the host that holds the stack for a caller another
/// host holds.
pub fn stack_bound_to_session_here(
    sessions_base: &Path,
    project_id: &str,
    session_id: &str,
) -> Result<Option<String>, StackCallerUnreadable> {
    // TODO(stack-node-reads): implement — scan the project's stacks in `<base>/projects`.
    let _ = (sessions_base, project_id, session_id);
    Ok(None)
}

/// The stack role of a caller another host holds, given the role its owner reports for it
/// (`ListSessions`' `session_role`) and the stack this host holds with a node bound to it. An
/// orchestrator role wins and owns no stack here; otherwise a bound node reads its own stack.
pub fn forwarded_stack_caller_role(
    listed_session_role: &str,
    bound_stack_id: Option<&str>,
) -> Option<StackCallerRole> {
    // TODO(stack-node-reads): implement.
    let _ = (listed_session_role, bound_stack_id);
    None
}

/// Every stack tool and stack git tool `role` is offered, stack tools first — what a session's
/// `ListExecTools` appends to its catalogue on the daemon and in its jail alike.
pub fn stack_and_git_tools_offered_to(role: Option<&StackCallerRole>) -> Vec<ToolDef> {
    // TODO(stack-node-reads): implement — `stack_tools_offered_to` then `stack_git_tools_offered_to`.
    let _ = role;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_changeset::stack_fixtures::{a_stack_host, stack_id_of, AStackHost, A_PROJECT};
    use tddy_core::changeset::{Changeset, Stack, StackNode};

    use super::*;
    use crate::stack_git_tools::STACK_GIT_TOOL_NAMES;
    use crate::stack_tools::STACK_TOOL_NAMES;

    fn a_stack_binding(child: Option<&str>) -> Stack {
        Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".to_string(),
                title: "Store".to_string(),
                session_id: child.map(str::to_string),
                ..StackNode::default()
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
        stack_id_of(&orch).expect("the orchestrator's changeset names its stack")
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

    // ── stack_caller_from_records ────────────────────────────────────────────────────────────

    #[test]
    fn a_session_its_orchestrators_stack_binds_is_classified_a_node_of_that_stack() {
        // Given an orchestrator whose node n1 is bound to a child session naming it
        let host = a_stack_host();
        let stack_id = an_orchestrator_binding(&host, Some("child"));
        a_child_of_orch(&host, "child");

        // When the child is classified from the host's records
        let caller = stack_caller_from_records(host.base(), "child").unwrap();

        // Then it is a node of that stack, in its project
        assert_eq!(
            caller,
            Some(StackCaller {
                session_id: "child".to_string(),
                project_id: A_PROJECT.to_string(),
                role: StackCallerRole::StackNode { stack_id },
            })
        );
    }

    #[test]
    fn an_orchestrator_role_session_is_classified_owning_the_stack_its_changeset_names() {
        // Given an orchestrator-role session whose stack the project's store holds
        let host = a_stack_host();
        let own = an_orchestrator_binding(&host, None);

        // When it is classified
        let caller = stack_caller_from_records(host.base(), "orch").unwrap();

        // Then it is an orchestrator owning that stack
        assert_eq!(
            caller.map(|c| c.role),
            Some(StackCallerRole::Orchestrator {
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
        let caller = stack_caller_from_records(host.base(), "child").unwrap();

        // Then it has no stack role
        assert_eq!(caller, None);
    }

    #[test]
    fn a_session_with_no_record_cannot_be_classified() {
        // Given a session id with nothing on disk
        let host = a_stack_host();

        // When it is classified
        let unreadable =
            stack_caller_from_records(host.base(), "ghost").expect_err("its role cannot be known");

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
        let stack_id = an_orchestrator_binding(&host, Some("held-elsewhere"));

        // When this host looks for a stack binding that session
        let bound = stack_bound_to_session_here(host.base(), A_PROJECT, "held-elsewhere").unwrap();

        // Then it finds that stack
        assert_eq!(bound, Some(stack_id));
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
            Some(StackCallerRole::Orchestrator { own_stack_id: None })
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
            Some(StackCallerRole::StackNode {
                stack_id: "stack-1".to_string()
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
            Some(StackCallerRole::Orchestrator { own_stack_id: None })
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
        let node = StackCallerRole::StackNode {
            stack_id: "stack-1".to_string(),
        };

        // When the stack tools it is offered are listed
        let offered = names(stack_and_git_tools_offered_to(Some(&node)));

        // Then they are the two reads alone
        assert_eq!(offered, vec!["pr_stack_status", "pr_read"]);
    }

    #[test]
    fn an_orchestrator_is_offered_every_stack_tool_then_every_stack_git_tool() {
        // Given an orchestrator
        let orchestrator = StackCallerRole::Orchestrator { own_stack_id: None };

        // When the stack tools it is offered are listed
        let offered = names(stack_and_git_tools_offered_to(Some(&orchestrator)));

        // Then they are the eleven stack tools followed by the six git tools
        let every: Vec<String> = STACK_TOOL_NAMES
            .iter()
            .chain(STACK_GIT_TOOL_NAMES.iter())
            .map(|name| name.to_string())
            .collect();
        assert_eq!(offered, every);
    }
}
