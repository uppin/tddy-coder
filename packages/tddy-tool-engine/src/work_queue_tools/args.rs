use crate::work_item_annotation_tools::WORK_ITEM_CLEAR_BADGE;
use crate::work_item_annotation_tools::WORK_ITEM_SET_BADGE;
use crate::work_item_annotation_tools::WORK_ITEM_SET_LABELS;
use crate::work_item_link_tools::WORK_ITEM_ASSIGN_PR;
use crate::work_item_link_tools::WORK_ITEM_LINK_SESSION;
use crate::work_item_link_tools::WORK_ITEM_UNASSIGN_PR;
use crate::work_item_link_tools::WORK_ITEM_UNLINK_SESSION;
use crate::work_queue_lineage_tools::WORK_QUEUE_GRAPH;
use crate::work_queue_lineage_tools::WORK_QUEUE_SET_DEPENDENCIES;
use crate::work_queue_waves_tools::WORK_QUEUE_WAVES;

use serde::Deserialize;

use super::WorkQueueOp;
use super::{
    PR_ADD_PLANNED, PR_ADOPT, PR_COMMENTS, PR_DELETE_PLANNED, PR_READ, PR_SEARCH, PR_SET_STATUS,
    PR_STACK_STATUS, PR_UPDATE_PLANNED,
};

use super::PrAddress;

/// Every argument any stack tool takes; each tool reads the ones it names and refuses the missing
/// ones it requires ([`required`]).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct StackToolArgs {
    pub(crate) queue_id: Option<String>,
    pub(crate) node_id: Option<String>,
    pub(crate) item_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) labels: Vec<String>,
    pub(crate) key: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) tone: Option<String>,
    pub(crate) needs: Option<Vec<String>>,
    pub(crate) pull_number: Option<u64>,
    pub(crate) include_files: bool,
    pub(crate) query: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) base: Option<String>,
    pub(crate) limit: u32,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) branch_suggestion: Option<String>,
    pub(crate) parents: Vec<String>,
    pub(crate) sync_pr: bool,
    pub(crate) kind: Option<String>,
    pub(crate) note: Option<String>,
    pub(crate) wave: Option<u32>,
    pub(crate) skip_review: bool,
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
pub(crate) fn work_queue_op_of(
    tool_name: &str,
    args: StackToolArgs,
) -> Result<WorkQueueOp, String> {
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
        crate::work_queue_schedule_tools::WORK_ITEM_SCHEDULE => WorkQueueOp::Schedule {
            item_id: required(tool_name, args.item_id, "item_id")?,
            kind: required(tool_name, args.kind, "kind")?,
            skip_review: args.skip_review,
        },
        crate::work_queue_schedule_tools::WORK_QUEUE_SCHEDULE_WAVE => WorkQueueOp::ScheduleWave {
            wave: required(tool_name, args.wave, "wave")?,
            skip_review: args.skip_review,
        },
        crate::work_queue_schedule_tools::WORK_ITEM_UNSCHEDULE => WorkQueueOp::Unschedule {
            item_id: required(tool_name, args.item_id, "item_id")?,
        },
        crate::work_queue_schedule_tools::WORK_QUEUE_SCHEDULES => WorkQueueOp::Schedules,
        other => return Err(format!("unknown stack tool: {other}")),
    })
}
