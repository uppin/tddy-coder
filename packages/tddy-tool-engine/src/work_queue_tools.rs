//! The PR-stack plan and read tools, served by the daemon and addressed by stack id
//! (`#orchestrators` 7/11).
//!
//! A stack is a project record ([`tddy_core::work_queue_store`]), not a session's: any session
//! that may plan the project's work names the stack it acts on with `queue_id`, and a stack another
//! host holds with [`TARGET_DAEMON_INSTANCE_ID`] — the daemon forwards that call whole to the host
//! that owns the stack, so the port below is only ever asked about this host's stacks.
//!
//! | Tool | WorkQueue | Writes |
//! |---|---|---|
//! | `work_queue_list` | — (the project's stacks on the host) | — |
//! | `work_queue_status` | required | derived statuses, wave, `chain_health` (leaks, line state) |
//! | `work_pr_read`, `work_pr_comments` | required for `node_id`, optional for `pull_number` | — |
//! | `work_pr_search` | optional (the project's repository) | — |
//! | `work_item_add`, `work_item_update`, `work_item_delete`, `work_item_set_status`, `work_item_adopt` | required | the store; `work_item_update` with `sync_pr` also the PR's title and body |
//!
//! **Who is offered them** follows the queue, never a role (`#workqueue` 5/14,
//! [`crate::work_queue_access`]): the session that owns a queue (its `queue_id` is that session's id)
//! has every tool on it; a session one of its items links to is *addressed* and may read it
//! (`work_queue_status`, `work_pr_read`); everyone else has nothing, and the daemon refuses its call.
//! A session addressing a session that owns no queue is refused with a hint naming how a queue
//! comes to exist. A read-only session may call all of them: they write only the queue and, for a
//! synced update, the PR on GitHub.
//!
//! GitHub is reached as the **project's account**, not the calling session's: a read-only session
//! holds no credential of its own.
//!
//! Feature doc: docs/ft/coder/pr-stacking.md § PR-management tools.
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-plan-tools.md.

pub mod caller_records;
mod defs;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tddy_core::work_queue_store::WorkQueueRecord;

use crate::work_item_annotation_tools::{
    WORK_ITEM_CLEAR_BADGE, WORK_ITEM_SET_BADGE, WORK_ITEM_SET_LABELS,
};
use crate::work_item_link_tools::{
    WORK_ITEM_ASSIGN_PR, WORK_ITEM_LINK_SESSION, WORK_ITEM_UNASSIGN_PR, WORK_ITEM_UNLINK_SESSION,
};
use crate::work_queue_access::WorkQueueAccess;
use crate::work_queue_lineage_tools::{WORK_QUEUE_GRAPH, WORK_QUEUE_SET_DEPENDENCIES};
use crate::work_queue_waves_tools::WORK_QUEUE_WAVES;
use crate::ToolOutcome;

pub const PR_STACK_LIST: &str = "work_queue_list";
pub const PR_STACK_STATUS: &str = "work_queue_status";
pub const PR_READ: &str = "work_pr_read";
pub const PR_SEARCH: &str = "work_pr_search";
pub const PR_COMMENTS: &str = "work_pr_comments";
pub const PR_ADD_PLANNED: &str = "work_item_add";
pub const PR_UPDATE_PLANNED: &str = "work_item_update";
pub const PR_DELETE_PLANNED: &str = "work_item_delete";
pub const PR_SET_STATUS: &str = "work_item_set_status";
pub const PR_ADOPT: &str = "work_item_adopt";

/// The argument naming the stack a tool acts on.
pub const STACK_ID: &str = "queue_id";

/// The ten tools, in the order they are advertised.
pub const WORK_QUEUE_TOOL_NAMES: [&str; 10] = [
    PR_STACK_LIST,
    PR_STACK_STATUS,
    PR_READ,
    PR_SEARCH,
    PR_COMMENTS,
    PR_ADD_PLANNED,
    PR_UPDATE_PLANNED,
    PR_DELETE_PLANNED,
    PR_SET_STATUS,
    PR_ADOPT,
];

/// Whether `name` is one of the eleven stack tools.
pub fn is_stack_tool(name: &str) -> bool {
    WORK_QUEUE_TOOL_NAMES.contains(&name)
        || crate::work_item_link_tools::WORK_ITEM_LINK_TOOL_NAMES.contains(&name)
        || crate::work_item_annotation_tools::WORK_ITEM_ANNOTATION_TOOL_NAMES.contains(&name)
        || crate::work_queue_lineage_tools::WORK_QUEUE_LINEAGE_TOOL_NAMES.contains(&name)
        || crate::work_queue_waves_tools::WORK_QUEUE_WAVES_TOOL_NAMES.contains(&name)
        || crate::work_item_commit_flow_tools::WORK_ITEM_COMMIT_FLOW_TOOL_NAMES.contains(&name)
}

/// Whether `name` is one of the two tools a node of a stack is offered for its own stack:
/// `work_queue_status` and `work_pr_read`.
pub fn is_stack_read_tool(name: &str) -> bool {
    matches!(name, PR_STACK_STATUS | PR_READ)
}

/// The session calling a work tool, as the daemon read it from its own records. What it may do is
/// decided per queue, from the queue ([`WorkQueueAccess`]), not from the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkQueueCaller {
    pub session_id: String,
    pub project_id: String,
}

/// Whether a session with `access` is offered (and served) the stack tool `tool_name`: the owner
/// every one, an addressed session the two reads.
pub fn is_stack_tool_offered_to(access: WorkQueueAccess, tool_name: &str) -> bool {
    is_stack_tool(tool_name)
        && match access {
            WorkQueueAccess::Owner => true,
            WorkQueueAccess::Addressed => is_stack_read_tool(tool_name),
        }
}

/// The queue `caller`'s call of `tool_name` acts on, given the `queue_id` it `named` (if any): the
/// named one, else the caller's own (a queue's id is its owning session's id). `Ok(None)` for
/// `work_queue_list`, which acts on no queue. Whether the caller may touch that queue is decided by
/// [`WorkQueueToolPort::access_to`], not here.
pub fn addressed_stack_id(
    caller: &WorkQueueCaller,
    tool_name: &str,
    named: Option<&str>,
) -> Result<Option<String>, String> {
    if tool_name == PR_STACK_LIST {
        return Ok(None);
    }
    let named = named.map(str::trim).filter(|id| !id.is_empty());
    Ok(Some(
        named.map_or_else(|| caller.session_id.clone(), str::to_string),
    ))
}

/// Which pull request a read names: the one a node of the stack records, or one by number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrAddress {
    Node(String),
    Number(u64),
}

/// Where a stack operation runs: the caller's project, and the stack it addressed, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkQueueScope {
    pub project_id: String,
    pub queue_id: Option<String>,
}

/// One stack operation, its arguments parsed and checked. The port does it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkQueueOp {
    /// `work_queue_status`: every node with its live GitHub state, derived statuses refreshed.
    Status,
    ReadPr {
        pr: PrAddress,
        include_files: bool,
    },
    SearchPrs {
        query: Option<String>,
        /// `open` unless named.
        state: String,
        author: Option<String>,
        base: Option<String>,
        /// `0` is "no preference".
        limit: u32,
    },
    PrComments {
        pr: PrAddress,
    },
    AddPlanned {
        title: String,
        description: String,
        branch_suggestion: Option<String>,
        parents: Vec<String>,
    },
    UpdatePlanned {
        node_id: String,
        title: Option<String>,
        description: Option<String>,
        branch_suggestion: Option<String>,
        sync_pr: bool,
    },
    DeletePlanned {
        node_id: String,
    },
    SetStatus {
        node_id: String,
        kind: String,
        note: Option<String>,
    },
    Adopt {
        pull_number: u64,
        parents: Vec<String>,
    },
    /// `work_item_assign_pr` (`#workqueue` 6/14).
    AssignPr {
        item_id: String,
        pull_number: u64,
    },
    /// `work_item_unassign_pr`.
    UnassignPr {
        item_id: String,
    },
    /// `work_item_link_session`.
    LinkSession {
        item_id: String,
        session_id: String,
    },
    /// `work_item_unlink_session`.
    UnlinkSession {
        item_id: String,
        session_id: String,
    },
    /// `work_item_set_labels` (`#workqueue` 7/14).
    SetLabels {
        item_id: String,
        labels: Vec<String>,
    },
    /// `work_item_set_badge`.
    SetBadge {
        item_id: String,
        key: String,
        text: String,
        tone: String,
    },
    /// `work_item_clear_badge`.
    ClearBadge {
        item_id: String,
        key: String,
    },
    /// `work_queue_set_dependencies` (`#workqueue` 9/14).
    SetDependencies {
        item_id: String,
        needs: Vec<String>,
    },
    /// `work_queue_graph`: line edges and stored dependency edges.
    Graph,
    /// `work_queue_waves` (`#workqueue` 10/14): waves, derived line, readiness, line check.
    Waves,
    /// `work_item_inbox` (`#workqueue` 11/14): the commits the item is missing, with sources and conflict
    /// predictions.
    Inbox {
        item_id: String,
    },
    /// `work_item_outbox` (`#workqueue` 11/14): per successor, what it lacks of this item.
    Outbox {
        item_id: String,
    },
}

/// What the tools need from the daemon: this host's stacks of a project, and the operations on one.
/// Every answer is about this host — a call naming another host was forwarded before it got here.
#[async_trait]
pub trait WorkQueueToolPort: Send + Sync {
    /// This host, as the eligible-daemon roster names it; every stack `work_queue_list` answers
    /// carries it.
    fn daemon_instance_id(&self) -> String;

    /// Every stack of `project_id` on this host.
    async fn list_stacks(&self, project_id: &str) -> Result<Vec<WorkQueueRecord>, String>;

    /// What `caller` may do with the queue `queue_id` (a session id) on this host: its access, or the
    /// refusal naming why it has none — the queue's session owns no queue (with the hint naming how
    /// one comes to exist), or the queue is not the caller's.
    async fn access_to(
        &self,
        caller: &WorkQueueCaller,
        queue_id: &str,
    ) -> Result<WorkQueueAccess, String>;

    /// Run `op` in `scope`, answering the tool's JSON. An unknown stack, an unknown node and a
    /// GitHub refusal are the error.
    async fn run_stack_op(&self, scope: &WorkQueueScope, op: WorkQueueOp) -> Result<Value, String>;
}

pub use crate::work_queue_tools::defs::*;

/// Run one of the ten work tools for `caller`.
///
/// Access is checked here against the addressed queue ([`WorkQueueToolPort::access_to`]) for a call that
/// acts on one; bad
/// arguments, a refusal of access and a refusal of the port come back as an `is_error` outcome
/// naming what was refused.
pub async fn execute_stack_tool(
    port: &dyn WorkQueueToolPort,
    caller: &WorkQueueCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    match run(port, caller, tool_name, args_json).await {
        Ok(answer) => ToolOutcome::ok(answer.to_string()),
        Err(message) => ToolOutcome::err(message),
    }
}

/// Every argument any stack tool takes; each tool reads the ones it names and refuses the missing
/// ones it requires ([`required`]).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StackToolArgs {
    queue_id: Option<String>,
    node_id: Option<String>,
    item_id: Option<String>,
    session_id: Option<String>,
    labels: Vec<String>,
    key: Option<String>,
    text: Option<String>,
    tone: Option<String>,
    needs: Option<Vec<String>>,
    pull_number: Option<u64>,
    include_files: bool,
    query: Option<String>,
    state: Option<String>,
    author: Option<String>,
    base: Option<String>,
    limit: u32,
    title: Option<String>,
    description: Option<String>,
    branch_suggestion: Option<String>,
    parents: Vec<String>,
    sync_pr: bool,
    kind: Option<String>,
    note: Option<String>,
}

/// The state a `work_pr_search` that expressed no preference is run with: the PRs still in play.
const DEFAULT_SEARCH_STATE: &str = "open";

/// `value`, or the refusal naming the argument `tool_name` requires.
fn required<T>(tool_name: &str, value: Option<T>, argument: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("{tool_name}: invalid arguments: missing field `{argument}`"))
}

/// The pull request a read names: exactly one of `node_id` and `pull_number`.
fn pr_address_of(tool_name: &str, args: &StackToolArgs) -> Result<PrAddress, String> {
    match (&args.node_id, args.pull_number) {
        (Some(node_id), None) => Ok(PrAddress::Node(node_id.clone())),
        (None, Some(number)) => Ok(PrAddress::Number(number)),
        _ => Err(format!(
            "{tool_name}: name exactly one of node_id and pull_number — naming neither or both is \
             rejected"
        )),
    }
}

/// The operation `tool_name` asks for, its arguments checked. The two project-level tools are not
/// operations on a stack and are answered by [`run`] itself.
fn work_queue_op_of(tool_name: &str, args: StackToolArgs) -> Result<WorkQueueOp, String> {
    Ok(match tool_name {
        PR_STACK_STATUS => WorkQueueOp::Status,
        PR_READ => WorkQueueOp::ReadPr {
            pr: pr_address_of(tool_name, &args)?,
            include_files: args.include_files,
        },
        PR_COMMENTS => WorkQueueOp::PrComments {
            pr: pr_address_of(tool_name, &args)?,
        },
        PR_SEARCH => WorkQueueOp::SearchPrs {
            query: args.query,
            state: args
                .state
                .unwrap_or_else(|| DEFAULT_SEARCH_STATE.to_string()),
            author: args.author,
            base: args.base,
            limit: args.limit,
        },
        PR_ADD_PLANNED => WorkQueueOp::AddPlanned {
            title: required(tool_name, args.title, "title")?,
            description: required(tool_name, args.description, "description")?,
            branch_suggestion: args.branch_suggestion,
            parents: args.parents,
        },
        PR_UPDATE_PLANNED => WorkQueueOp::UpdatePlanned {
            node_id: required(tool_name, args.node_id, "node_id")?,
            title: args.title,
            description: args.description,
            branch_suggestion: args.branch_suggestion,
            sync_pr: args.sync_pr,
        },
        PR_DELETE_PLANNED => WorkQueueOp::DeletePlanned {
            node_id: required(tool_name, args.node_id, "node_id")?,
        },
        PR_SET_STATUS => WorkQueueOp::SetStatus {
            node_id: required(tool_name, args.node_id, "node_id")?,
            kind: required(tool_name, args.kind, "kind")?,
            note: args.note,
        },
        PR_ADOPT => WorkQueueOp::Adopt {
            pull_number: required(tool_name, args.pull_number, "pull_number")?,
            parents: args.parents,
        },
        WORK_ITEM_ASSIGN_PR => WorkQueueOp::AssignPr {
            item_id: required(tool_name, args.item_id, "item_id")?,
            pull_number: required(tool_name, args.pull_number, "pull_number")?,
        },
        WORK_ITEM_UNASSIGN_PR => WorkQueueOp::UnassignPr {
            item_id: required(tool_name, args.item_id, "item_id")?,
        },
        WORK_ITEM_LINK_SESSION => WorkQueueOp::LinkSession {
            item_id: required(tool_name, args.item_id, "item_id")?,
            session_id: required(tool_name, args.session_id, "session_id")?,
        },
        WORK_ITEM_UNLINK_SESSION => WorkQueueOp::UnlinkSession {
            item_id: required(tool_name, args.item_id, "item_id")?,
            session_id: required(tool_name, args.session_id, "session_id")?,
        },
        WORK_ITEM_SET_LABELS => WorkQueueOp::SetLabels {
            item_id: required(tool_name, args.item_id, "item_id")?,
            labels: args.labels,
        },
        WORK_ITEM_SET_BADGE => WorkQueueOp::SetBadge {
            item_id: required(tool_name, args.item_id, "item_id")?,
            key: required(tool_name, args.key, "key")?,
            text: required(tool_name, args.text, "text")?,
            tone: required(tool_name, args.tone, "tone")?,
        },
        WORK_ITEM_CLEAR_BADGE => WorkQueueOp::ClearBadge {
            item_id: required(tool_name, args.item_id, "item_id")?,
            key: required(tool_name, args.key, "key")?,
        },
        WORK_QUEUE_SET_DEPENDENCIES => WorkQueueOp::SetDependencies {
            item_id: required(tool_name, args.item_id, "item_id")?,
            needs: required(tool_name, args.needs, "needs")?,
        },
        WORK_QUEUE_GRAPH => WorkQueueOp::Graph,
        WORK_QUEUE_WAVES => WorkQueueOp::Waves,
        crate::work_item_commit_flow_tools::WORK_ITEM_INBOX => WorkQueueOp::Inbox {
            item_id: required(tool_name, args.item_id, "item_id")?,
        },
        crate::work_item_commit_flow_tools::WORK_ITEM_OUTBOX => WorkQueueOp::Outbox {
            item_id: required(tool_name, args.item_id, "item_id")?,
        },
        other => return Err(format!("unknown stack tool: {other}")),
    })
}

async fn run(
    port: &dyn WorkQueueToolPort,
    caller: &WorkQueueCaller,
    tool_name: &str,
    args_json: &str,
) -> Result<Value, String> {
    let args: StackToolArgs = if args_json.trim().is_empty() {
        StackToolArgs::default()
    } else {
        serde_json::from_str(args_json)
            .map_err(|e| format!("{tool_name}: invalid arguments: {e}"))?
    };
    let refused = |why: String| format!("{tool_name}: {why}");
    match tool_name {
        PR_STACK_LIST => {
            let stacks = port
                .list_stacks(&caller.project_id)
                .await
                .map_err(refused)?;
            let host = port.daemon_instance_id();
            Ok(json!({ "stacks": stacks.iter().map(|record| json!({
                "queue_id": record.queue_id,
                "title": record.title,
                "daemon_instance_id": host,
                "owner_session_id": record.owner_session_id(),
                "created_at_unix_ms": record.created_at_unix_ms,
                "node_count": record.stack.nodes.len(),
            })).collect::<Vec<_>>() }))
        }
        _ => {
            let named = args.queue_id.clone();
            let queue_id = addressed_stack_id(caller, tool_name, named.as_deref())?;
            let op = work_queue_op_of(tool_name, args)?;
            let reads_a_node = matches!(
                op,
                WorkQueueOp::ReadPr {
                    pr: PrAddress::Node(_),
                    ..
                } | WorkQueueOp::PrComments {
                    pr: PrAddress::Node(_)
                }
            );
            // A search or a read by pull number needs no queue; it is served to a session that owns one.
            let acts_on_no_queue = !reads_a_node
                && named.as_deref().is_none_or(|id| id.trim().is_empty())
                && matches!(tool_name, PR_SEARCH | PR_READ | PR_COMMENTS);
            // A call that acts on no queue was gated by the caller's own host, on what its own
            // records offer it; one that acts on a queue is gated here, by the queue.
            if !acts_on_no_queue {
                let queue = queue_id.as_deref().unwrap_or(&caller.session_id);
                let access = port.access_to(caller, queue).await.map_err(refused)?;
                if !is_stack_tool_offered_to(access, tool_name) {
                    return Err(refused(format!(
                        "not offered to a session that is only addressed by work queue {queue}: it may read its status and its pull requests only"
                    )));
                }
            }
            let scope = WorkQueueScope {
                project_id: caller.project_id.clone(),
                queue_id: if acts_on_no_queue { None } else { queue_id },
            };
            port.run_stack_op(&scope, op).await.map_err(refused)
        }
    }
}

#[cfg(test)]
mod tests {
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

        async fn run_stack_op(
            &self,
            scope: &WorkQueueScope,
            op: WorkQueueOp,
        ) -> Result<Value, String> {
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
}
