use std::sync::Mutex;

use pretty_assertions::assert_eq;

use super::*;

const OWN_STACK: &str = "orch";
const OTHER_STACK: &str = "stack-other";

fn an_orchestrator_owning_a_stack() -> WorkQueueCaller {
    WorkQueueCaller {
        session_id: "orch".to_string(),
        project_id: "proj".to_string(),
    }
}

// ── who is offered them ───────────────────────────────────────────────────────────────────

#[test]
fn the_owner_is_offered_every_git_tool_and_an_addressed_session_none() {
    // When each git tool is checked for each access
    let offered = |access| -> Vec<&str> {
        WORK_ITEM_GIT_TOOL_NAMES
            .into_iter()
            .filter(|tool| is_stack_git_tool_offered_to(access, tool))
            .collect()
    };

    // Then the owner has all six, in advertised order, and an addressed session none
    assert_eq!(
        (
            offered(WorkQueueAccess::Owner),
            offered(WorkQueueAccess::Addressed)
        ),
        (WORK_ITEM_GIT_TOOL_NAMES.to_vec(), Vec::<&str>::new())
    );
}

#[test]
fn a_plan_tool_is_not_a_git_tool_the_owner_is_offered_here() {
    assert!(!is_stack_git_tool_offered_to(
        WorkQueueAccess::Owner,
        "work_item_add"
    ));
}

// ── definitions ───────────────────────────────────────────────────────────────────────────

#[test]
fn every_git_tool_names_its_node_its_stack_and_its_host() {
    // When the definitions are read
    let defs = stack_git_tool_defs();

    // Then each requires the item it acts on (node_id; item_id for propagate; the wave range for
    // land) and accepts queue_id and daemon_instance_id
    assert_eq!(
        defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        WORK_ITEM_GIT_TOOL_NAMES.to_vec()
    );
    for def in &defs {
        let schema: Value = serde_json::from_str(&def.input_schema_json).unwrap();
        let names = match def.name.as_str() {
            WORK_ITEM_PROPAGATE => "item_id",
            WORK_QUEUE_LAND => "from_wave",
            _ => "node_id",
        };
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!(names)),
            "{} requires {names}",
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
    ops: Mutex<Vec<(String, WorkItemGitOp)>>,
    refusal: Option<String>,
}

impl RecordingPort {
    fn refusing(why: &str) -> Self {
        Self {
            refusal: Some(why.to_string()),
            ..Self::default()
        }
    }

    fn ops(&self) -> Vec<(String, WorkItemGitOp)> {
        self.ops.lock().unwrap().clone()
    }
}

#[async_trait]
impl WorkItemGitToolPort for RecordingPort {
    async fn access_to(
        &self,
        _caller: &WorkQueueCaller,
        _queue_id: &str,
    ) -> Result<WorkQueueAccess, String> {
        Ok(WorkQueueAccess::Owner)
    }

    async fn run_stack_git_op(
        &self,
        _caller: &WorkQueueCaller,
        queue_id: &str,
        op: WorkItemGitOp,
    ) -> Result<Value, String> {
        if let Some(why) = &self.refusal {
            return Err(why.clone());
        }
        self.ops.lock().unwrap().push((queue_id.to_string(), op));
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

fn op(queue_id: &str, op: WorkItemGitOp) -> (String, WorkItemGitOp) {
    (queue_id.to_string(), op)
}

#[tokio::test]
async fn work_item_merge_is_run_on_the_named_stack_for_its_node() {
    // Given an orchestrator naming another stack of its project
    let port = RecordingPort::default();

    // When it merges a node of that stack
    let outcome = run(
        &port,
        PR_MERGE,
        json!({ "queue_id": OTHER_STACK, "node_id": "n1" }),
    )
    .await;

    // Then the port merged that node of the named stack
    assert!(!outcome.is_error, "{}", outcome.error_message);
    assert_eq!(
        port.ops(),
        vec![op(
            OTHER_STACK,
            WorkItemGitOp::Merge {
                node_id: "n1".to_string()
            }
        )]
    );
}

#[tokio::test]
async fn work_item_repoint_naming_no_stack_acts_on_the_callers_own() {
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
            WorkItemGitOp::Repoint {
                node_id: "n2".to_string(),
                new_base: None
            }
        )]
    );
}

#[tokio::test]
async fn work_item_set_parents_with_an_empty_list_makes_the_node_a_root() {
    // Given an orchestrator
    let port = RecordingPort::default();

    // When it gives a node an empty parent list
    let outcome = run(
        &port,
        PR_SET_PARENTS,
        json!({ "queue_id": OWN_STACK, "node_id": "n2", "parents": [] }),
    )
    .await;

    // Then the port is asked to make it a root
    assert!(!outcome.is_error, "{}", outcome.error_message);
    assert_eq!(
        port.ops(),
        vec![op(
            OWN_STACK,
            WorkItemGitOp::SetParents {
                node_id: "n2".to_string(),
                parents: Vec::new()
            }
        )]
    );
}

#[tokio::test]
async fn work_item_set_parents_without_parents_is_refused_before_the_port_is_asked() {
    // Given an orchestrator
    let port = RecordingPort::default();

    // When it moves a node but forgets its parents
    let outcome = run(
        &port,
        PR_SET_PARENTS,
        json!({ "queue_id": OWN_STACK, "node_id": "n2" }),
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
async fn work_item_resolve_conflicts_with_delegate_asks_the_port_to_delegate() {
    // Given an orchestrator
    let port = RecordingPort::default();

    // When it checks a node's conflicts, delegating their resolution
    let outcome = run(
        &port,
        PR_RESOLVE_CONFLICTS,
        json!({ "queue_id": OWN_STACK, "node_id": "n1", "delegate": true }),
    )
    .await;

    // Then the port is asked to check and delegate
    assert!(!outcome.is_error, "{}", outcome.error_message);
    assert_eq!(
        port.ops(),
        vec![op(
            OWN_STACK,
            WorkItemGitOp::ResolveConflicts {
                node_id: "n1".to_string(),
                delegate: true
            }
        )]
    );
}

#[tokio::test]
async fn work_item_resolve_conflicts_does_not_delegate_unless_asked() {
    // Given an orchestrator
    let port = RecordingPort::default();

    // When it checks a node's conflicts naming no delegate
    run(
        &port,
        PR_RESOLVE_CONFLICTS,
        json!({ "queue_id": OWN_STACK, "node_id": "n1" }),
    )
    .await;

    // Then nothing is delegated
    assert_eq!(
        port.ops(),
        vec![op(
            OWN_STACK,
            WorkItemGitOp::ResolveConflicts {
                node_id: "n1".to_string(),
                delegate: false
            }
        )]
    );
}

#[tokio::test]
async fn work_item_spawn_session_carries_the_type_and_model_it_names() {
    // Given an orchestrator
    let port = RecordingPort::default();

    // When it starts a node's session naming a type and a model
    run(
        &port,
        PR_SPAWN_CHILD,
        json!({
            "queue_id": OWN_STACK,
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
            WorkItemGitOp::SpawnChild {
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
    let outcome = run(&port, PR_MERGE, json!({ "queue_id": OWN_STACK })).await;

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
        json!({ "queue_id": OWN_STACK, "node_id": "n1" }),
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
    let outcome = run(&port, "work_item_add", json!({ "title": "x" })).await;

    // Then it is refused naming the tool, and nothing ran
    assert!(outcome.is_error);
    assert!(
        outcome.error_message.contains("work_item_add"),
        "{}",
        outcome.error_message
    );
    assert_eq!(port.ops(), Vec::new());
}
