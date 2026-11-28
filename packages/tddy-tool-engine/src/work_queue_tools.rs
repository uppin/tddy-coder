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
use serde_json::{json, Value};
use tddy_core::work_queue_store::WorkQueueRecord;

#[cfg(test)]
use crate::work_item_annotation_tools::WORK_ITEM_CLEAR_BADGE;
#[cfg(test)]
use crate::work_item_annotation_tools::WORK_ITEM_SET_BADGE;
#[cfg(test)]
use crate::work_item_annotation_tools::WORK_ITEM_SET_LABELS;
use crate::work_queue_access::WorkQueueAccess;
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
        || crate::work_item_file_tools::WORK_ITEM_FILE_TOOL_NAMES.contains(&name)
        || crate::work_queue_lineage_tools::WORK_QUEUE_LINEAGE_TOOL_NAMES.contains(&name)
        || crate::work_queue_waves_tools::WORK_QUEUE_WAVES_TOOL_NAMES.contains(&name)
        || crate::work_item_commit_flow_tools::WORK_ITEM_COMMIT_FLOW_TOOL_NAMES.contains(&name)
        || crate::work_queue_schedule_tools::WORK_QUEUE_SCHEDULE_TOOL_NAMES.contains(&name)
}

/// Whether `name` is one of the tools a node of a stack is offered for its own stack:
/// `work_queue_status` and `work_pr_read`, and — since work item files — the list of what its
/// item carries.
pub fn is_stack_read_tool(name: &str) -> bool {
    matches!(name, PR_STACK_STATUS | PR_READ)
        || name == crate::work_item_file_tools::WORK_ITEM_LIST_FILES
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
        /// The daemon holding the session; empty for the queue's own host.
        daemon_instance_id: String,
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
    /// `work_item_schedule` (`#workqueue` 13/14): arm a merge or a propagation on an item.
    Schedule {
        item_id: String,
        /// `merge` or `propagate`.
        kind: String,
        skip_review: bool,
    },
    /// `work_queue_schedule_wave` (`#workqueue` 13/14): arm a merge on each open-PR item of a wave.
    ScheduleWave {
        wave: u32,
        skip_review: bool,
    },
    /// `work_item_unschedule` (`#workqueue` 13/14).
    Unschedule {
        item_id: String,
    },
    /// `work_queue_schedules` (`#workqueue` 13/14): the armed schedules with live conditions.
    Schedules,
    /// `work_item_attach_file`: record one file on an item, from a source this host can read.
    AttachFile {
        item_id: String,
        basename: String,
        source: FileSource,
    },
    /// `work_item_detach_file`: drop one file from an item.
    DetachFile {
        item_id: String,
        basename: String,
    },
    /// `work_item_list_files`: what an item carries.
    ListFiles {
        item_id: String,
    },
}

/// Where an attach's bytes come from — a file the queue-owner host can already read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSource {
    /// A path under the queue owner's own session's artifacts.
    ArtifactRelativePath(String),
    /// A path inside the project's main repository.
    RepoFilePath(String),
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

mod args;
#[cfg(test)]
use args::work_queue_op_of;

async fn run(
    port: &dyn WorkQueueToolPort,
    caller: &WorkQueueCaller,
    tool_name: &str,
    args_json: &str,
) -> Result<Value, String> {
    let args: args::StackToolArgs = if args_json.trim().is_empty() {
        args::StackToolArgs::default()
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
            let op = args::work_queue_op_of(tool_name, args)?;
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
mod tests;
