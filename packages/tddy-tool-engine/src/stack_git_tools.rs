//! The PR-stack git tools, run by the daemon in each node's own worktree (`#orchestrators` 10/11).
//!
//! The siblings of the plan and read tools ([`crate::stack_tools`], `#orchestrators` 7/11): the
//! same `stack_id` addressing, the same [`StackCaller`] classification and the same forwarding of
//! a call naming another host with [`TARGET_DAEMON_INSTANCE_ID`] — but every one of these six
//! changes git or a pull request, so they are offered to an **orchestrator** alone, never to a node
//! of a stack.
//!
//! | Tool | Git, on the daemon | GitHub, as the project's account |
//! |---|---|---|
//! | `pr_merge` | — | merges the node's PR; the node reads `merged` |
//! | `pr_close` | — | closes the node's PR; the node reads `closed` |
//! | `pr_repoint` | rebases the node's branch onto its effective base (or `new_base`) in the node's worktree, force-pushes with lease to the project's remote | retargets the PR's base |
//! | `pr_set_parents` | as `pr_repoint`, after the parents change | as `pr_repoint` |
//! | `pr_resolve_conflicts` | tries the base merge against the node's branch, leaves the worktree as it was | — ; `delegate` asks the node's session to resolve the paths |
//! | `pr_spawn_child` | the node's worktree is created by the daemon's start path | — |
//!
//! Git never runs in the caller's checkout: a read-only orchestrator has none to run it in. A node
//! whose worktree is dirty or whose session is running is refused a rebase, naming why, and left
//! untouched.
//!
//! Feature doc: docs/ft/coder/pr-stacking.md § Merge and repoint.
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-git-tools.md.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, TARGET_DAEMON_INSTANCE_ID};
use crate::stack_tools::{StackCaller, StackCallerRole, STACK_ID};
use crate::ToolOutcome;

pub const PR_MERGE: &str = "pr_merge";
pub const PR_CLOSE: &str = "pr_close";
pub const PR_REPOINT: &str = "pr_repoint";
pub const PR_SET_PARENTS: &str = "pr_set_parents";
pub const PR_RESOLVE_CONFLICTS: &str = "pr_resolve_conflicts";
pub const PR_SPAWN_CHILD: &str = "pr_spawn_child";

/// The six tools, in the order they are advertised.
pub const STACK_GIT_TOOL_NAMES: [&str; 6] = [
    PR_MERGE,
    PR_CLOSE,
    PR_REPOINT,
    PR_SET_PARENTS,
    PR_RESOLVE_CONFLICTS,
    PR_SPAWN_CHILD,
];

/// Whether `name` is one of the six stack git tools.
pub fn is_stack_git_tool(name: &str) -> bool {
    STACK_GIT_TOOL_NAMES.contains(&name)
}

/// Whether `role` is offered (and served) the stack git tool `tool_name`: an orchestrator every
/// one, a node of a stack none.
pub fn is_stack_git_tool_offered_to(role: &StackCallerRole, tool_name: &str) -> bool {
    // TODO(stack-git-tools): implement — `Orchestrator` and `is_stack_git_tool`.
    let _ = (role, tool_name);
    false
}

/// The stack git tools `role` is offered, in [`STACK_GIT_TOOL_NAMES`] order; none for a session
/// with no stack role and none for a node.
pub fn stack_git_tools_offered_to(role: Option<&StackCallerRole>) -> Vec<ToolDef> {
    // TODO(stack-git-tools): implement — `stack_git_tool_defs` filtered by
    // `is_stack_git_tool_offered_to`.
    let _ = role;
    Vec::new()
}

/// One stack git operation on a node of the addressed stack, its arguments parsed and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackGitOp {
    /// `pr_merge`: merge the node's open PR; the node reads `merged`.
    Merge { node_id: String },
    /// `pr_close`: close the node's open PR without merging; the node reads `closed`.
    Close { node_id: String },
    /// `pr_repoint`: rebase the node's branch onto `new_base` — its effective base when `None` — in
    /// its worktree, force-push it with lease, and retarget its PR.
    Repoint {
        node_id: String,
        new_base: Option<String>,
    },
    /// `pr_set_parents`: give the node a whole new parent list, then realign it as `Repoint` does
    /// when it owns a branch.
    SetParents {
        node_id: String,
        parents: Vec<String>,
    },
    /// `pr_resolve_conflicts`: the paths the node's branch conflicts on against its base; the node
    /// reads `has-conflicts` while there are any. With `delegate`, the node's session is asked to
    /// resolve them — a running one by a new turn, a stopped one resumed with that message.
    ResolveConflicts { node_id: String, delegate: bool },
    /// `pr_spawn_child`: start the node's session through the daemon's start path, its stack
    /// parent the caller, on the node's suggested branch. `session_type` and `model` default to the
    /// caller's own.
    SpawnChild {
        node_id: String,
        session_type: Option<String>,
        model: Option<String>,
    },
}

/// What the git tools need from the daemon: one operation on a node of one of this host's stacks.
/// A call naming another host was forwarded before it got here.
#[async_trait]
pub trait StackGitToolPort: Send + Sync {
    /// Run `op` on `stack_id` of `caller`'s project for `caller`, answering the tool's JSON. An
    /// unknown stack or node, a node refused a rebase (dirty worktree, running session) and a git or
    /// GitHub failure are the error, each naming what was refused.
    async fn run_stack_git_op(
        &self,
        caller: &StackCaller,
        stack_id: &str,
        op: StackGitOp,
    ) -> Result<Value, String>;
}

/// The six tools' definitions. Every one takes [`STACK_ID`] and [`TARGET_DAEMON_INSTANCE_ID`] and
/// names the node it acts on by `node_id`.
pub fn stack_git_tool_defs() -> Vec<ToolDef> {
    let node_id = |what: &str| json!({ "type": "string", "description": what });
    vec![
        tool_def(
            PR_MERGE,
            "Merge a stack node's open pull request into its base, as the project's GitHub account, and mark the node merged. Then pr_repoint its dependents.",
            node_object(json!({ "node_id": node_id("Stack node id whose PR to merge.") }), &["node_id"]),
        ),
        tool_def(
            PR_CLOSE,
            "Close a stack node's open pull request without merging, as the project's GitHub account, and mark the node closed.",
            node_object(json!({ "node_id": node_id("Stack node id whose PR to close.") }), &["node_id"]),
        ),
        tool_def(
            PR_REPOINT,
            "Rebase a stack node's branch onto its effective base (the next unmerged ancestor, else the project's default branch) or onto new_base, in the node's own worktree on the daemon, force-push it with lease to the project's remote, and retarget its pull request's base. Refused, with nothing touched, while the node's worktree has uncommitted changes or its session is running.",
            node_object(
                json!({
                    "node_id": node_id("Stack node id to repoint."),
                    "new_base": { "type": "string", "description": "Base branch to repoint onto; the node's effective base when omitted." }
                }),
                &["node_id"],
            ),
        ),
        tool_def(
            PR_SET_PARENTS,
            "Move a node in the stack by giving it a whole new parent list. parents is required: pass [] to make the node a root off the stack bottom. Rejects an unknown parent, self-parenthood, a duplicate entry, and any change that would close a cycle, writing nothing. When the node owns a branch, it is realigned as pr_repoint does: rebased in its own worktree, force-pushed with lease to the project's remote, its pull request retargeted — refused, with nothing written, while its worktree is dirty or its session is running.",
            node_object(
                json!({
                    "node_id": node_id("Stack node id to move."),
                    "parents": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "The node's whole new parent list (chosen ancestors); pass [] to make it a root off the stack bottom. Required — there is no default."
                    }
                }),
                &["node_id", "parents"],
            ),
        ),
        tool_def(
            PR_RESOLVE_CONFLICTS,
            "Find the files a stack node's branch conflicts on against its base, on the daemon, leaving the node's worktree as it was. Marks the node has-conflicts while there are any and clears that once its branch merges cleanly. With delegate, the node's session is asked to resolve the named files: a running session is given the request as a new turn, a stopped one is resumed with it.",
            node_object(
                json!({
                    "node_id": node_id("Stack node id whose branch to check."),
                    "delegate": { "type": "boolean", "description": "Ask the node's session to resolve the conflicting files (resuming it when stopped)." }
                }),
                &["node_id"],
            ),
        ),
        tool_def(
            PR_SPAWN_CHILD,
            "Start the coding session for a planned stack node, on its suggested branch, with the calling session recorded as its orchestrator. session_type and model default to the caller's own. Answers the new session's session_id.",
            node_object(
                json!({
                    "node_id": node_id("Stack node id to start a session for."),
                    "session_type": { "type": "string", "description": "Session type to start; the caller's own when omitted." },
                    "model": { "type": "string", "description": "Model to start it with; the caller's own when omitted." }
                }),
                &["node_id"],
            ),
        ),
    ]
}

/// A tool schema of `properties` plus the stack and the host arguments.
fn node_object(properties: Value, required: &[&str]) -> Value {
    let mut properties = properties;
    properties[STACK_ID] = json!({
        "type": "string",
        "description": "The stack, as pr_stack_list or pr_stack_create answers its stack_id. The calling session's own stack (the one its changeset names) when omitted."
    });
    properties[TARGET_DAEMON_INSTANCE_ID] = json!({
        "type": "string",
        "description": "The host that holds the stack, as pr_stack_list reports it in its daemon_instance_id; empty for this host."
    });
    json!({ "type": "object", "properties": properties, "required": required })
}

/// Run one of the six stack git tools for `caller`.
///
/// The daemon has already refused a caller its role does not entitle. The arguments are parsed
/// first — bad arguments are refused before the stack is resolved — then the stack is resolved as
/// the plan tools resolve it ([`crate::stack_tools::addressed_stack_id`]), then the port runs the
/// operation. Each refusal comes back as an `is_error` outcome naming what was refused.
pub async fn execute_stack_git_tool(
    port: &dyn StackGitToolPort,
    caller: &StackCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    // TODO(stack-git-tools): implement — parse the arguments into a `StackGitOp`, resolve the stack
    // with `addressed_stack_id`, run it with `run_stack_git_op`.
    let _ = (port, caller, args_json);
    ToolOutcome::err(format!("{tool_name}: not implemented on this host yet"))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use pretty_assertions::assert_eq;

    use super::*;

    const OWN_STACK: &str = "stack-own";
    const OTHER_STACK: &str = "stack-other";

    fn an_orchestrator_owning_a_stack() -> StackCaller {
        StackCaller {
            session_id: "orch".to_string(),
            project_id: "proj".to_string(),
            role: StackCallerRole::Orchestrator {
                own_stack_id: Some(OWN_STACK.to_string()),
            },
        }
    }

    fn a_node() -> StackCallerRole {
        StackCallerRole::StackNode {
            stack_id: OWN_STACK.to_string(),
        }
    }

    // ── who is offered them ───────────────────────────────────────────────────────────────────

    #[test]
    fn an_orchestrator_is_offered_every_git_tool() {
        // Given an orchestrator
        let role = an_orchestrator_owning_a_stack().role;

        // When the git tools it is offered are listed
        let offered: Vec<String> = stack_git_tools_offered_to(Some(&role))
            .into_iter()
            .map(|def| def.name)
            .collect();

        // Then it is offered all six, in advertised order
        assert_eq!(offered, STACK_GIT_TOOL_NAMES.map(str::to_string).to_vec());
    }

    #[test]
    fn a_node_of_a_stack_is_offered_no_git_tool() {
        // Given a session bound to a node of a stack
        let role = a_node();

        // When each git tool is checked for it
        let offered: Vec<&str> = STACK_GIT_TOOL_NAMES
            .into_iter()
            .filter(|tool| is_stack_git_tool_offered_to(&role, tool))
            .collect();

        // Then none is offered
        assert_eq!(offered, Vec::<&str>::new());
        assert_eq!(stack_git_tools_offered_to(Some(&role)), Vec::new());
    }

    #[test]
    fn a_session_with_no_stack_role_is_offered_no_git_tool() {
        assert_eq!(stack_git_tools_offered_to(None), Vec::new());
    }

    #[test]
    fn a_plan_tool_is_not_a_git_tool_an_orchestrator_is_offered_here() {
        // Given an orchestrator
        let role = an_orchestrator_owning_a_stack().role;

        // Then this family serves only its own six names
        assert!(!is_stack_git_tool_offered_to(&role, "pr_add_planned"));
    }

    // ── definitions ───────────────────────────────────────────────────────────────────────────

    #[test]
    fn every_git_tool_names_its_node_its_stack_and_its_host() {
        // When the definitions are read
        let defs = stack_git_tool_defs();

        // Then each of the six requires node_id and accepts stack_id and daemon_instance_id
        assert_eq!(
            defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            STACK_GIT_TOOL_NAMES.to_vec()
        );
        for def in &defs {
            let schema: Value = serde_json::from_str(&def.input_schema_json).unwrap();
            assert!(
                schema["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("node_id")),
                "{} requires node_id",
                def.name
            );
            for property in [STACK_ID, TARGET_DAEMON_INSTANCE_ID] {
                assert!(
                    schema["properties"].get(property).is_some(),
                    "{} accepts {property}",
                    def.name
                );
            }
        }
    }

    // ── execution over a port ─────────────────────────────────────────────────────────────────

    /// A port that records every operation it is asked and answers each with `{"ran": true}`, or
    /// refuses every one with `refusal`.
    #[derive(Default)]
    struct RecordingPort {
        ops: Mutex<Vec<(String, StackGitOp)>>,
        refusal: Option<String>,
    }

    impl RecordingPort {
        fn refusing(why: &str) -> Self {
            Self {
                refusal: Some(why.to_string()),
                ..Self::default()
            }
        }

        fn ops(&self) -> Vec<(String, StackGitOp)> {
            self.ops.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl StackGitToolPort for RecordingPort {
        async fn run_stack_git_op(
            &self,
            _caller: &StackCaller,
            stack_id: &str,
            op: StackGitOp,
        ) -> Result<Value, String> {
            if let Some(why) = &self.refusal {
                return Err(why.clone());
            }
            self.ops.lock().unwrap().push((stack_id.to_string(), op));
            Ok(json!({ "ran": true }))
        }
    }

    async fn run(port: &RecordingPort, tool: &str, args: Value) -> ToolOutcome {
        execute_stack_git_tool(
            port,
            &an_orchestrator_owning_a_stack(),
            tool,
            &args.to_string(),
        )
        .await
    }

    fn op(stack_id: &str, op: StackGitOp) -> (String, StackGitOp) {
        (stack_id.to_string(), op)
    }

    #[tokio::test]
    async fn pr_merge_is_run_on_the_named_stack_for_its_node() {
        // Given an orchestrator naming another stack of its project
        let port = RecordingPort::default();

        // When it merges a node of that stack
        let outcome = run(
            &port,
            PR_MERGE,
            json!({ "stack_id": OTHER_STACK, "node_id": "n1" }),
        )
        .await;

        // Then the port merged that node of the named stack
        assert!(!outcome.is_error, "{}", outcome.error_message);
        assert_eq!(
            port.ops(),
            vec![op(
                OTHER_STACK,
                StackGitOp::Merge {
                    node_id: "n1".to_string()
                }
            )]
        );
    }

    #[tokio::test]
    async fn pr_repoint_naming_no_stack_acts_on_the_callers_own() {
        // Given an orchestrator owning a stack
        let port = RecordingPort::default();

        // When it repoints a node naming no stack and no new base
        let outcome = run(&port, PR_REPOINT, json!({ "node_id": "n2" })).await;

        // Then its own stack's node is repointed onto its effective base
        assert!(!outcome.is_error, "{}", outcome.error_message);
        assert_eq!(
            port.ops(),
            vec![op(
                OWN_STACK,
                StackGitOp::Repoint {
                    node_id: "n2".to_string(),
                    new_base: None
                }
            )]
        );
    }

    #[tokio::test]
    async fn pr_set_parents_with_an_empty_list_makes_the_node_a_root() {
        // Given an orchestrator
        let port = RecordingPort::default();

        // When it gives a node an empty parent list
        let outcome = run(
            &port,
            PR_SET_PARENTS,
            json!({ "stack_id": OWN_STACK, "node_id": "n2", "parents": [] }),
        )
        .await;

        // Then the port is asked to make it a root
        assert!(!outcome.is_error, "{}", outcome.error_message);
        assert_eq!(
            port.ops(),
            vec![op(
                OWN_STACK,
                StackGitOp::SetParents {
                    node_id: "n2".to_string(),
                    parents: Vec::new()
                }
            )]
        );
    }

    #[tokio::test]
    async fn pr_set_parents_without_parents_is_refused_before_the_port_is_asked() {
        // Given an orchestrator
        let port = RecordingPort::default();

        // When it moves a node but forgets its parents
        let outcome = run(
            &port,
            PR_SET_PARENTS,
            json!({ "stack_id": OWN_STACK, "node_id": "n2" }),
        )
        .await;

        // Then the call is refused as missing parents — an omission never reads as "make it a root"
        assert!(outcome.is_error);
        assert!(
            outcome.error_message.contains("missing") && outcome.error_message.contains("parents"),
            "{}",
            outcome.error_message
        );
        assert_eq!(port.ops(), Vec::new());
    }

    #[tokio::test]
    async fn pr_resolve_conflicts_with_delegate_asks_the_port_to_delegate() {
        // Given an orchestrator
        let port = RecordingPort::default();

        // When it checks a node's conflicts, delegating their resolution
        let outcome = run(
            &port,
            PR_RESOLVE_CONFLICTS,
            json!({ "stack_id": OWN_STACK, "node_id": "n1", "delegate": true }),
        )
        .await;

        // Then the port is asked to check and delegate
        assert!(!outcome.is_error, "{}", outcome.error_message);
        assert_eq!(
            port.ops(),
            vec![op(
                OWN_STACK,
                StackGitOp::ResolveConflicts {
                    node_id: "n1".to_string(),
                    delegate: true
                }
            )]
        );
    }

    #[tokio::test]
    async fn pr_resolve_conflicts_does_not_delegate_unless_asked() {
        // Given an orchestrator
        let port = RecordingPort::default();

        // When it checks a node's conflicts naming no delegate
        run(
            &port,
            PR_RESOLVE_CONFLICTS,
            json!({ "stack_id": OWN_STACK, "node_id": "n1" }),
        )
        .await;

        // Then nothing is delegated
        assert_eq!(
            port.ops(),
            vec![op(
                OWN_STACK,
                StackGitOp::ResolveConflicts {
                    node_id: "n1".to_string(),
                    delegate: false
                }
            )]
        );
    }

    #[tokio::test]
    async fn pr_spawn_child_carries_the_type_and_model_it_names() {
        // Given an orchestrator
        let port = RecordingPort::default();

        // When it starts a node's session naming a type and a model
        run(
            &port,
            PR_SPAWN_CHILD,
            json!({
                "stack_id": OWN_STACK,
                "node_id": "n1",
                "session_type": "claude-cli",
                "model": "claude-opus-4-8",
            }),
        )
        .await;

        // Then the port is asked to start it with those
        assert_eq!(
            port.ops(),
            vec![op(
                OWN_STACK,
                StackGitOp::SpawnChild {
                    node_id: "n1".to_string(),
                    session_type: Some("claude-cli".to_string()),
                    model: Some("claude-opus-4-8".to_string()),
                }
            )]
        );
    }

    #[tokio::test]
    async fn a_git_tool_missing_its_node_is_refused_as_invalid_arguments() {
        // Given an orchestrator
        let port = RecordingPort::default();

        // When it merges naming no node
        let outcome = run(&port, PR_MERGE, json!({ "stack_id": OWN_STACK })).await;

        // Then the call is refused as missing node_id, and nothing ran
        assert!(outcome.is_error);
        assert!(
            outcome.error_message.contains("missing") && outcome.error_message.contains("node_id"),
            "{}",
            outcome.error_message
        );
        assert_eq!(port.ops(), Vec::new());
    }

    #[tokio::test]
    async fn the_ports_refusal_is_the_tools_error() {
        // Given a port refusing a running node's rebase
        let why = "session child-1 is running: stop it before its branch is rebased";
        let port = RecordingPort::refusing(why);

        // When the orchestrator repoints that node
        let outcome = run(
            &port,
            PR_REPOINT,
            json!({ "stack_id": OWN_STACK, "node_id": "n1" }),
        )
        .await;

        // Then the tool's error is the port's refusal
        assert!(outcome.is_error);
        assert!(
            outcome.error_message.contains(why),
            "{}",
            outcome.error_message
        );
    }

    #[tokio::test]
    async fn a_tool_outside_the_family_is_refused() {
        // Given an orchestrator
        let port = RecordingPort::default();

        // When a plan tool is run through the git family
        let outcome = run(&port, "pr_add_planned", json!({ "title": "x" })).await;

        // Then it is refused naming the tool, and nothing ran
        assert!(outcome.is_error);
        assert!(
            outcome.error_message.contains("pr_add_planned"),
            "{}",
            outcome.error_message
        );
        assert_eq!(port.ops(), Vec::new());
    }
}
