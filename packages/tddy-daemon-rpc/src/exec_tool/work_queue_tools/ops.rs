//! The stack tools' operations over one record of the host's stack store.
//!
//! Synchronous — the plan writers read and rewrite a record under its lock and the reads call
//! GitHub — so the port runs [`run`] on a blocking thread. Each answers the JSON `tddy-tools` gave
//! the same tool when it ran against a session's own stack, now addressed by stack id.

use std::path::PathBuf;

use serde_json::{json, Value};
use tddy_core::changeset::{resolve_stack_node_branch, WorkQueue};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::work_queue_store::{WorkQueueRecord, WorkQueueStore};
use tddy_tool_engine::work_queue_tools::{PrAddress, WorkQueueOp};
use tddy_work_queue::internal_status::{derive_internal_status, reconcile_internal_status};
use tddy_work_queue::item_ops::item_annotations::{self, WorkItemBadge};
use tddy_work_queue::item_ops::item_links;
use tddy_work_queue::item_ops::{
    add_work_item, adopt_pr_as_work_item, delete_work_item, AddWorkItemInput, StoredWorkQueue,
    WorkQueueHome,
};
use tddy_work_queue::pr_insight::{self, PrSearchInput};

use super::github::GithubSource;
use super::shape::{work_pr_comments_json, work_pr_read_json, work_pr_search_json};

mod lineage_ops;
mod waves_ops;
pub(super) use lineage_ops::graph_json;
use lineage_ops::set_dependencies;
use waves_ops::{queue_waves_of, waves_answer};

/// The stack an operation acts on, and where the host keeps it.
#[derive(Clone)]
pub(super) struct OpEnv {
    pub(super) store: WorkQueueStore,
    pub(super) project_id: String,
    /// The stack the call addressed; `None` for a tool that needs none (`work_pr_search`, a read by
    /// number).
    pub(super) queue_id: Option<String>,
    /// The caller's sessions, where a node's child session is read.
    pub(super) sessions_base: PathBuf,
}

impl OpEnv {
    fn queue_id(&self) -> Result<&str, String> {
        self.queue_id
            .as_deref()
            .ok_or_else(|| "the call names no stack".to_string())
    }

    fn home(&self) -> Result<StoredWorkQueue<'_>, String> {
        Ok(StoredWorkQueue {
            store: &self.store,
            project_id: &self.project_id,
            queue_id: self.queue_id()?,
        })
    }

    fn record(&self) -> Result<WorkQueueRecord, String> {
        self.store
            .read(&self.project_id, self.queue_id()?)
            .map_err(|e| e.to_string())
    }

    pub(super) fn stack(&self) -> Result<WorkQueue, String> {
        self.record().map(|record| record.stack)
    }

    /// The pull request a read names: the one a node of the stack records, or the number itself.
    fn pull_number(&self, pr: &PrAddress) -> Result<u64, String> {
        match pr {
            PrAddress::Number(number) => Ok(*number),
            PrAddress::Node(node_id) => pr_insight::pull_number_for_node(&self.stack()?, node_id),
        }
    }
}

/// Run `op` on the stack `env` names, reaching GitHub through `github` when the operation needs it.
pub(super) fn run(env: &OpEnv, github: &GithubSource, op: WorkQueueOp) -> Result<Value, String> {
    match op {
        // `work_queue_status` reads GitHub through the inbox and so runs async, ahead of this blocking
        // dispatcher: see `DaemonStackTools::run_stack_op`.
        WorkQueueOp::Status => {
            Err("work_queue_status is served through the GitHub inbox".to_string())
        }
        WorkQueueOp::ReadPr { pr, include_files } => {
            let number = env.pull_number(&pr)?;
            let gh = github.client()?;
            let view = pr_insight::read_pr(&*gh, number, include_files)
                .map_err(|e| format!("read pr: {e}"))?;
            Ok(work_pr_read_json(&view))
        }
        WorkQueueOp::PrComments { pr } => {
            let number = env.pull_number(&pr)?;
            let gh = github.client()?;
            let view = pr_insight::read_pr_comments(&*gh, number)
                .map_err(|e| format!("read pr comments: {e}"))?;
            Ok(work_pr_comments_json(&view))
        }
        WorkQueueOp::SearchPrs {
            query,
            state,
            author,
            base,
            limit,
        } => {
            // The repository is the project's, resolved here: the agent's input names none, so a
            // search can never reach another one.
            let repo = github.repo()?;
            let gh = github.client()?;
            let hits = pr_insight::search_repository_prs(
                &*gh,
                &repo,
                PrSearchInput {
                    text: query,
                    state,
                    author,
                    base,
                    // `0` is "no preference" to `search_repository_prs`, which owns the default
                    // and the cap.
                    limit,
                },
            )
            .map_err(|e| format!("search prs: {e}"))?;
            Ok(work_pr_search_json(&hits))
        }
        WorkQueueOp::AddPlanned {
            title,
            description,
            branch_suggestion,
            parents,
        } => {
            let node = add_work_item(
                &env.home()?,
                AddWorkItemInput {
                    title,
                    description,
                    branch_suggestion,
                    parents,
                    child_recipe: None,
                },
            )?;
            Ok(json!({ "node_id": node.node_id }))
        }
        WorkQueueOp::UpdatePlanned {
            node_id,
            title,
            description,
            branch_suggestion,
            sync_pr,
        } => item_edits::update_planned(
            env,
            github,
            node_id,
            title,
            description,
            branch_suggestion,
            sync_pr,
        ),
        WorkQueueOp::DeletePlanned { node_id } => {
            let deleted = delete_work_item(&env.home()?, &node_id)?;
            Ok(json!({
                "deleted": deleted.node.node_id,
                "reparented_children": deleted.reparented_children,
                "orphaned_branch": deleted.orphaned_branch,
                "orphaned_session_id": deleted.orphaned_session_id,
            }))
        }
        WorkQueueOp::SetStatus {
            node_id,
            kind,
            note,
        } => item_edits::set_status(env, &node_id, kind, note),
        WorkQueueOp::Adopt {
            pull_number,
            parents,
        } => {
            let gh = github.client()?;
            let node = adopt_pr_as_work_item(&env.home()?, pull_number, parents, &*gh)?;
            Ok(json!({
                "node_id": node.node_id,
                "branch": node.branch,
                "parents": node.parents,
                "pr_status": node.pr_status.as_ref().map(|s| s.phase.clone()),
            }))
        }
        WorkQueueOp::AssignPr {
            item_id,
            pull_number,
        } => {
            // The PR is read as the project's account, so the item records the PR's own URL and head
            // branch — never the agent's claim of them.
            let gh = github.client()?;
            let pr = gh
                .get_pr(pull_number)
                .map_err(|e| format!("read pull request #{pull_number}: {e}"))?;
            let assigned = item_links::AssignedPr {
                number: pr.number,
                url: pr.url,
                head_branch: pr.head_branch,
            };
            annotation_edits::edit_links(env, &item_id, |queue| {
                item_links::assign_pr(queue, &item_id, assigned)
            })
        }
        WorkQueueOp::UnassignPr { item_id } => {
            annotation_edits::edit_links(env, &item_id, |queue| {
                item_links::unassign_pr(queue, &item_id)
            })
        }
        WorkQueueOp::LinkSession {
            item_id,
            session_id,
            daemon_instance_id,
        } => annotation_edits::edit_links(env, &item_id, |queue| {
            item_links::link_session(
                queue,
                &item_id,
                item_links::LinkedSession {
                    session_id,
                    daemon_instance_id: daemon_instance_id.trim().to_string(),
                },
            )
        }),
        WorkQueueOp::UnlinkSession {
            item_id,
            session_id,
        } => annotation_edits::edit_links(env, &item_id, |queue| {
            item_links::unlink_session(queue, &item_id, &session_id)
        }),
        WorkQueueOp::SetLabels { item_id, labels } => {
            annotation_edits::edit_annotations(env, &item_id, |queue| {
                item_annotations::set_labels(queue, &item_id, labels)
            })
        }
        WorkQueueOp::SetBadge {
            item_id,
            key,
            text,
            tone,
        } => {
            let tone = crate::work_queue::parse_tone(&tone)
                .map_err(|status| status.message().to_string())?;
            annotation_edits::edit_annotations(env, &item_id, |queue| {
                item_annotations::set_badge(queue, &item_id, WorkItemBadge { key, text, tone })
            })
        }
        WorkQueueOp::ClearBadge { item_id, key } => {
            annotation_edits::edit_annotations(env, &item_id, |queue| {
                item_annotations::clear_badge(queue, &item_id, &key)
            })
        }
        WorkQueueOp::SetDependencies { item_id, needs } => set_dependencies(env, &item_id, needs),
        WorkQueueOp::Waves => waves_answer(&env.stack()?),
        // Served through the GitHub inbox, ahead of this blocking dispatcher.
        WorkQueueOp::Schedule { .. }
        | WorkQueueOp::ScheduleWave { .. }
        | WorkQueueOp::Unschedule { .. }
        | WorkQueueOp::Schedules => {
            Err("the schedule tools are served through the GitHub inbox".to_string())
        }
        WorkQueueOp::Inbox { .. } | WorkQueueOp::Outbox { .. } => Err(
            "work_item_inbox and work_item_outbox are served through the GitHub inbox".to_string(),
        ),
        WorkQueueOp::Graph => {
            Err("work_queue_graph is served through the GitHub inbox".to_string())
        }
    }
}

mod annotation_edits;

/// Answer the node summaries of the stack as stored — read after the statuses were refreshed, so the
/// answer reflects what is stored. Each node lists its linked sessions with their hosts, and its
/// `branch` is the effective one (stored, else a linked session's, else its PR's): resolved for
/// display, never written back. A refresh failure is surfaced as `refresh_error`, never hidden.
pub(super) fn status_answer(env: &OpEnv, refresh_error: Option<String>) -> Result<Value, String> {
    // Read after the possible persist, so the answer reflects what is stored.
    let stack = env.stack()?;
    // A cycle in hand-edited edges leaves wave 0 and no binding needs rather than failing the status.
    let derived = queue_waves_of(&stack).ok();
    let nodes: Vec<Value> = stack
        .nodes
        .iter()
        .map(|n| {
            let (wave, binding_needs) = waves_ops::wave_and_needs(derived.as_ref(), &n.node_id);
            // The item's actual branch: what it records, else a linked session's, else its PR's.
            // Resolved for display only — a fallback-derived branch is never written back.
            let branch = resolve_stack_node_branch(&env.sessions_base, n)
                .or_else(|| n.pr.as_ref().map(|pr| pr.head_branch.clone()));
            json!({
                "wave": wave,
                "binding_needs": binding_needs,
                "node_id": n.node_id,
                "title": n.title,
                "branch": branch,
                "session_id": n.session_id,
                "linked_sessions": n.linked_sessions.iter().map(|s| json!({
                    "session_id": s.session_id,
                    "daemon_instance_id": s.daemon_instance_id,
                })).collect::<Vec<_>>(),
                "pr_status": n.pr_status.as_ref().map(|s| s.phase.clone()),
                "internal_status": n.internal_status.as_ref().map(|s| json!({
                    "kind": s.kind,
                    "note": s.note,
                    "source": s.source,
                })),
            })
        })
        .collect();
    let mut out = json!({ "nodes": nodes });
    if let Some(err) = refresh_error {
        out["refresh_error"] = Value::String(err);
    }
    Ok(out)
}

/// What a status refresh needs from the stored queue before it reads GitHub: the queue, the directory
/// holding the spawn-request markers of a node not yet started (the owning session's), and the
/// project's default branch.
pub(super) struct RefreshInputs {
    pub(super) stack: WorkQueue,
    pub(super) planner_dir: PathBuf,
    pub(super) default_branch: String,
}

pub(super) fn refresh_inputs(env: &OpEnv, github: &GithubSource) -> Result<RefreshInputs, String> {
    let record = env.record()?;
    let default_branch = github.default_branch()?;
    // The spawn-request markers of a node not yet started live in the session that owns the queue.
    let planner_dir = unified_session_dir_path(&env.sessions_base, record.owner_session_id());
    Ok(RefreshInputs {
        stack: record.stack,
        planner_dir,
        default_branch,
    })
}

/// Derive internal statuses from the assembled `views` and persist them (override-wins).
pub(super) fn persist_derived_statuses(
    env: &OpEnv,
    views: &[tddy_work_queue::assess::NodeView],
    default_branch: &str,
) -> Result<(), String> {
    let derived = derive_internal_status(views, default_branch);
    env.home()?
        .update_stack(|stack| {
            for (node_id, derived) in &derived {
                if let Some(node) = stack.nodes.iter_mut().find(|n| &n.node_id == node_id) {
                    node.internal_status = Some(reconcile_internal_status(
                        node.internal_status.as_ref(),
                        derived.clone(),
                    ));
                }
            }
        })
        .map_err(|e| format!("persist derived statuses: {e}"))
}

mod item_edits;
