use std::sync::Mutex;

use pretty_assertions::assert_eq;
use tddy_core::changeset::WorkQueue;

use super::*;

const OWN_QUEUE: &str = "caller";
const OTHER_QUEUE: &str = "someone-else";

fn a_caller() -> WorkQueueCaller {
    WorkQueueCaller {
        session_id: OWN_QUEUE.to_string(),
        project_id: "proj".to_string(),
    }
}

// ── annotation tools (`#workqueue` 7/14) ──────────────────────────────────────────────────

fn parsed(tool: &str, args: serde_json::Value) -> Result<WorkQueueOp, String> {
    work_queue_op_of(tool, serde_json::from_value(args).expect("args"))
}

#[test]
fn the_annotation_tools_parse_into_their_ops() {
    assert_eq!(
        (
            parsed(
                WORK_ITEM_SET_LABELS,
                json!({ "item_id": "n1", "labels": ["api", "ui"] })
            ),
            parsed(
                WORK_ITEM_SET_BADGE,
                json!({ "item_id": "n1", "key": "qa", "text": "ok", "tone": "success" })
            ),
            parsed(
                WORK_ITEM_CLEAR_BADGE,
                json!({ "item_id": "n1", "key": "qa" })
            ),
        ),
        (
            Ok(WorkQueueOp::SetLabels {
                item_id: "n1".to_string(),
                labels: vec!["api".to_string(), "ui".to_string()],
            }),
            Ok(WorkQueueOp::SetBadge {
                item_id: "n1".to_string(),
                key: "qa".to_string(),
                text: "ok".to_string(),
                tone: "success".to_string(),
            }),
            Ok(WorkQueueOp::ClearBadge {
                item_id: "n1".to_string(),
                key: "qa".to_string(),
            }),
        )
    );
}

#[test]
fn a_badge_without_its_tone_is_refused_naming_the_argument() {
    let refused = parsed(
        WORK_ITEM_SET_BADGE,
        json!({ "item_id": "n1", "key": "qa", "text": "ok" }),
    )
    .expect_err("a tone is required");

    assert!(
        refused.contains("tone"),
        "refusal names the argument: {refused}"
    );
}

// ── addressing ────────────────────────────────────────────────────────────────────────────

#[test]
fn a_call_naming_no_queue_acts_on_the_callers_own() {
    assert_eq!(
        addressed_stack_id(&a_caller(), PR_ADD_PLANNED, None),
        Ok(Some(OWN_QUEUE.to_string()))
    );
}

#[test]
fn a_call_naming_a_queue_acts_on_it() {
    assert_eq!(
        addressed_stack_id(&a_caller(), PR_STACK_STATUS, Some(OTHER_QUEUE)),
        Ok(Some(OTHER_QUEUE.to_string()))
    );
}

#[test]
fn the_listing_addresses_no_queue() {
    assert_eq!(
        addressed_stack_id(&a_caller(), PR_STACK_LIST, None),
        Ok(None)
    );
}

#[test]
fn the_owner_is_served_every_tool_and_an_addressed_session_the_two_reads() {
    let served = |access| -> Vec<&str> {
        WORK_QUEUE_TOOL_NAMES
            .into_iter()
            .filter(|tool| is_stack_tool_offered_to(access, tool))
            .collect()
    };

    assert_eq!(
        (
            served(WorkQueueAccess::Owner),
            served(WorkQueueAccess::Addressed)
        ),
        (
            WORK_QUEUE_TOOL_NAMES.to_vec(),
            vec![PR_STACK_STATUS, PR_READ]
        )
    );
}

// ── execution over a port ─────────────────────────────────────────────────────────────────

/// A port that records every operation it is asked and answers each with `{"ran": true}`;
/// `access` is what every queue grants the caller (`None`: the refusal).
struct RecordingPort {
    ops: Mutex<Vec<(WorkQueueScope, WorkQueueOp)>>,
    access: Option<WorkQueueAccess>,
    refusal: Option<String>,
}

impl RecordingPort {
    fn granting(access: WorkQueueAccess) -> Self {
        Self {
            ops: Mutex::default(),
            access: Some(access),
            refusal: None,
        }
    }

    fn refusing(why: &str) -> Self {
        Self {
            refusal: Some(why.to_string()),
            ..Self::granting(WorkQueueAccess::Owner)
        }
    }

    fn without_access() -> Self {
        Self {
            access: None,
            ..Self::granting(WorkQueueAccess::Owner)
        }
    }

    fn ops(&self) -> Vec<(WorkQueueScope, WorkQueueOp)> {
        self.ops.lock().unwrap().clone()
    }
}

fn a_record(queue_id: &str, nodes: usize) -> WorkQueueRecord {
    WorkQueueRecord {
        queue_id: queue_id.to_string(),
        project_id: "proj".to_string(),
        created_at_unix_ms: 1_700_000_000_000,
        title: (queue_id == OWN_QUEUE).then(|| "Orchestrator sessions".to_string()),
        stack: WorkQueue {
            version: 1,
            nodes: (0..nodes)
                .map(|i| tddy_core::changeset::WorkItem {
                    node_id: format!("n{}", i + 1),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
    }
}

#[async_trait]
impl WorkQueueToolPort for RecordingPort {
    fn daemon_instance_id(&self) -> String {
        "host-a".to_string()
    }

    async fn list_stacks(&self, _project_id: &str) -> Result<Vec<WorkQueueRecord>, String> {
        Ok(vec![a_record(OWN_QUEUE, 2), a_record(OTHER_QUEUE, 0)])
    }

    async fn access_to(
        &self,
        _caller: &WorkQueueCaller,
        queue_id: &str,
    ) -> Result<WorkQueueAccess, String> {
        self.access
            .ok_or_else(|| format!("session {queue_id} has no work queue"))
    }

    async fn run_stack_op(&self, scope: &WorkQueueScope, op: WorkQueueOp) -> Result<Value, String> {
        if let Some(why) = &self.refusal {
            return Err(why.clone());
        }
        self.ops.lock().unwrap().push((scope.clone(), op));
        Ok(json!({ "ran": true }))
    }
}

fn answer_of(outcome: &ToolOutcome) -> Value {
    assert!(!outcome.is_error, "{}", outcome.error_message);
    serde_json::from_str(&outcome.result_json).expect("the answer is JSON")
}

fn own_scope() -> WorkQueueScope {
    WorkQueueScope {
        project_id: "proj".to_string(),
        queue_id: Some(OWN_QUEUE.to_string()),
    }
}

#[tokio::test]
async fn work_queue_list_answers_every_queue_with_the_host_that_holds_it() {
    // Given a host holding two queues, and an owner calling
    let port = RecordingPort::granting(WorkQueueAccess::Owner);

    // When it lists them
    let outcome = execute_stack_tool(&port, &a_caller(), PR_STACK_LIST, "{}").await;

    // Then each is listed with its id, title, host, owner, creation time and item count
    assert_eq!(
        answer_of(&outcome),
        json!({ "stacks": [
            {
                "queue_id": OWN_QUEUE,
                "title": "Orchestrator sessions",
                "daemon_instance_id": "host-a",
                "owner_session_id": OWN_QUEUE,
                "created_at_unix_ms": 1_700_000_000_000u64,
                "node_count": 2,
            },
            {
                "queue_id": OTHER_QUEUE,
                "title": null,
                "daemon_instance_id": "host-a",
                "owner_session_id": OTHER_QUEUE,
                "created_at_unix_ms": 1_700_000_000_000u64,
                "node_count": 0,
            },
        ] })
    );
}

#[tokio::test]
async fn a_session_with_no_access_to_the_addressed_queue_is_refused_with_the_ports_reason() {
    // Given a queue the caller has no access to
    let port = RecordingPort::without_access();

    // When it reads the status of another session's queue
    let outcome = execute_stack_tool(
        &port,
        &a_caller(),
        PR_STACK_STATUS,
        &json!({ "queue_id": OTHER_QUEUE }).to_string(),
    )
    .await;

    // Then the refusal is the port's, prefixed by the tool, and nothing ran
    assert_eq!(
        outcome.error_message,
        "work_queue_status: session someone-else has no work queue"
    );
    assert_eq!(port.ops(), Vec::new());
}

#[tokio::test]
async fn an_addressed_session_may_read_but_not_write() {
    // Given an addressed caller
    let port = RecordingPort::granting(WorkQueueAccess::Addressed);

    // When it reads status and tries to add an item
    let status = execute_stack_tool(&port, &a_caller(), PR_STACK_STATUS, "{}").await;
    let add = execute_stack_tool(
        &port,
        &a_caller(),
        PR_ADD_PLANNED,
        &json!({ "title": "t", "description": "d" }).to_string(),
    )
    .await;

    // Then the read ran and the write was refused
    assert_eq!((status.is_error, add.is_error), (false, true));
    assert_eq!(port.ops().len(), 1);
}

#[tokio::test]
async fn work_item_add_is_run_on_the_addressed_queue_with_its_arguments() {
    // Given an owner
    let port = RecordingPort::granting(WorkQueueAccess::Owner);

    // When it plans an item on its own queue
    let outcome = execute_stack_tool(
        &port,
        &a_caller(),
        PR_ADD_PLANNED,
        &json!({ "title": "Tools", "description": "the tools", "parents": ["n1"] }).to_string(),
    )
    .await;

    // Then the port planned it there, and its answer is the tool's
    assert_eq!(answer_of(&outcome), json!({ "ran": true }));
    assert_eq!(
        port.ops(),
        vec![(
            own_scope(),
            WorkQueueOp::AddPlanned {
                title: "Tools".to_string(),
                description: "the tools".to_string(),
                branch_suggestion: None,
                parents: vec!["n1".to_string()],
            }
        )]
    );
}

#[tokio::test]
async fn work_pr_search_names_open_when_no_state_is_given_and_acts_on_no_queue() {
    // Given an owner
    let port = RecordingPort::granting(WorkQueueAccess::Owner);

    // When it searches by text alone
    execute_stack_tool(
        &port,
        &a_caller(),
        PR_SEARCH,
        &json!({ "query": "store" }).to_string(),
    )
    .await;

    // Then the search is of open PRs in the project, with no queue and no limit preference
    assert_eq!(
        port.ops(),
        vec![(
            WorkQueueScope {
                project_id: "proj".to_string(),
                queue_id: None,
            },
            WorkQueueOp::SearchPrs {
                query: Some("store".to_string()),
                state: "open".to_string(),
                author: None,
                base: None,
                limit: 0,
            }
        )]
    );
}

#[tokio::test]
async fn work_pr_read_by_node_is_run_as_a_node_address() {
    // Given an addressed session
    let port = RecordingPort::granting(WorkQueueAccess::Addressed);

    // When it reads a node's PR with its files
    execute_stack_tool(
        &port,
        &a_caller(),
        PR_READ,
        &json!({ "node_id": "n2", "include_files": true }).to_string(),
    )
    .await;

    // Then the port reads that node's PR on the queue
    assert_eq!(
        port.ops(),
        vec![(
            own_scope(),
            WorkQueueOp::ReadPr {
                pr: PrAddress::Node("n2".to_string()),
                include_files: true,
            }
        )]
    );
}

#[tokio::test]
async fn a_read_naming_both_a_node_and_a_pull_number_is_refused_before_the_port_is_asked() {
    // Given an owner
    let port = RecordingPort::granting(WorkQueueAccess::Owner);

    // When it reads a PR naming both a node and a number
    let outcome = execute_stack_tool(
        &port,
        &a_caller(),
        PR_COMMENTS,
        &json!({ "queue_id": OWN_QUEUE, "node_id": "n1", "pull_number": 7 }).to_string(),
    )
    .await;

    // Then it is refused, and nothing was asked
    assert!(outcome.is_error, "{}", outcome.result_json);
    assert!(
        outcome.error_message.contains("exactly one"),
        "{}",
        outcome.error_message
    );
    assert_eq!(port.ops(), Vec::new());
}

#[tokio::test]
async fn a_planning_call_missing_its_title_is_refused_as_invalid_arguments() {
    // Given an owner
    let port = RecordingPort::granting(WorkQueueAccess::Owner);

    // When it plans an item with no title
    let outcome = execute_stack_tool(
        &port,
        &a_caller(),
        PR_ADD_PLANNED,
        &json!({ "description": "the tools" }).to_string(),
    )
    .await;

    // Then the arguments are refused, naming the tool, and nothing was asked
    assert!(outcome.is_error, "{}", outcome.result_json);
    assert!(
        outcome
            .error_message
            .starts_with("work_item_add: invalid arguments"),
        "{}",
        outcome.error_message
    );
    assert_eq!(port.ops(), Vec::new());
}

#[tokio::test]
async fn the_ports_refusal_is_the_tools_error() {
    // Given a port that refuses, naming why
    let port = RecordingPort::refusing("project proj holds no queue caller");

    // When an item is deleted from the queue
    let outcome = execute_stack_tool(
        &port,
        &a_caller(),
        PR_DELETE_PLANNED,
        &json!({ "node_id": "n1" }).to_string(),
    )
    .await;

    // Then the tool's error is the port's reason, prefixed by the tool
    assert!(outcome.is_error, "{}", outcome.result_json);
    assert_eq!(
        outcome.error_message,
        "work_item_delete: project proj holds no queue caller"
    );
}
