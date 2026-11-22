//! The stack tools' operations over one record of the host's stack store.
//!
//! Synchronous — the plan writers read and rewrite a record under its lock and the reads call
//! GitHub — so the port runs [`run`] on a blocking thread. Each answers the JSON `tddy-tools` gave
//! the same tool when it ran against a session's own stack, now addressed by stack id.

use std::path::PathBuf;

use serde_json::{json, Value};
use tddy_core::changeset::{PrInternalStatus, WorkQueue};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::work_queue_store::{WorkQueueRecord, WorkQueueStore};
use tddy_tool_engine::work_queue_tools::{PrAddress, WorkQueueOp};
use tddy_work_queue::internal_status::{derive_internal_status, reconcile_internal_status};
use tddy_work_queue::item_ops::item_annotations::{self, WorkItemBadge};
use tddy_work_queue::item_ops::item_links;
use tddy_work_queue::item_ops::{
    add_work_item, adopt_pr_as_work_item, delete_work_item, sync_node_to_github_pr,
    update_work_item, AddWorkItemInput, StoredWorkQueue, UpdateWorkItemInput, WorkQueueHome,
};
use tddy_work_queue::lineage;
use tddy_work_queue::pr_insight::{self, PrSearchInput};

use super::github::GithubSource;
use super::shape::{work_pr_comments_json, work_pr_read_json, work_pr_search_json};

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
        } => update_planned(
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
        } => set_status(env, &node_id, kind, note),
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
            edit_links(env, &item_id, |queue| {
                item_links::assign_pr(queue, &item_id, assigned)
            })
        }
        WorkQueueOp::UnassignPr { item_id } => edit_links(env, &item_id, |queue| {
            item_links::unassign_pr(queue, &item_id)
        }),
        WorkQueueOp::LinkSession {
            item_id,
            session_id,
        } => edit_links(env, &item_id, |queue| {
            item_links::link_session(
                queue,
                &item_id,
                item_links::LinkedSession {
                    session_id,
                    daemon_instance_id: String::new(),
                },
            )
        }),
        WorkQueueOp::UnlinkSession {
            item_id,
            session_id,
        } => edit_links(env, &item_id, |queue| {
            item_links::unlink_session(queue, &item_id, &session_id)
        }),
        WorkQueueOp::SetLabels { item_id, labels } => edit_annotations(env, &item_id, |queue| {
            item_annotations::set_labels(queue, &item_id, labels)
        }),
        WorkQueueOp::SetBadge {
            item_id,
            key,
            text,
            tone,
        } => {
            let tone = crate::work_queue::parse_tone(&tone)
                .map_err(|status| status.message().to_string())?;
            edit_annotations(env, &item_id, |queue| {
                item_annotations::set_badge(queue, &item_id, WorkItemBadge { key, text, tone })
            })
        }
        WorkQueueOp::ClearBadge { item_id, key } => edit_annotations(env, &item_id, |queue| {
            item_annotations::clear_badge(queue, &item_id, &key)
        }),
        WorkQueueOp::SetDependencies { item_id, needs } => set_dependencies(env, &item_id, needs),
        // Served through the GitHub inbox, ahead of this blocking dispatcher.
        WorkQueueOp::Graph => {
            Err("work_queue_graph is served through the GitHub inbox".to_string())
        }
    }
}

/// A refused annotation edit as the tool reports it — the same words the RPC uses.
fn annotation_refusal_message(error: item_annotations::AnnotationError) -> String {
    use item_annotations::AnnotationError::{NoSuchItem, TooLong, TooMany};
    match error {
        NoSuchItem(item) => format!("item '{item}' not found"),
        TooMany { limit } => format!("too many: the limit is {limit}"),
        TooLong { limit } => format!("too long: the limit is {limit} characters"),
    }
}

/// Apply an annotation `edit` to the queue under its record lock; a refused edit writes nothing and
/// is the error, and the answer is the item's labels and badges as stored.
fn edit_annotations(
    env: &OpEnv,
    item_id: &str,
    edit: impl FnOnce(&mut WorkQueue) -> Result<(), item_annotations::AnnotationError>,
) -> Result<Value, String> {
    let mut refused = None;
    env.home()?
        .update_stack(|queue| {
            let mut edited = queue.clone();
            match edit(&mut edited) {
                Ok(()) => *queue = edited,
                Err(error) => refused = Some(error),
            }
        })
        .map_err(|e| format!("annotate: {e}"))?;
    if let Some(error) = refused {
        return Err(annotation_refusal_message(error));
    }
    let stack = env.stack()?;
    Ok(json!({
        "item_id": item_id,
        "labels": item_annotations::labels_of(&stack, item_id),
        "badges": item_annotations::badges_of(&stack, item_id)
            .into_iter()
            .map(|b| json!({ "key": b.key, "text": b.text, "tone": b.tone }))
            .collect::<Vec<_>>(),
    }))
}

/// `work_queue_set_dependencies`: replace what `item_id` needs under the record lock; a refused edit
/// (unknown item, a cycle) writes nothing and is the error.
fn set_dependencies(env: &OpEnv, item_id: &str, needs: Vec<String>) -> Result<Value, String> {
    let mut refused = None;
    env.home()?
        .update_stack(|queue| {
            let mut edited = queue.clone();
            match lineage::set_dependencies(&mut edited, item_id, needs) {
                Ok(()) => *queue = edited,
                Err(error) => refused = Some(error),
            }
        })
        .map_err(|e| format!("set dependencies: {e}"))?;
    if let Some(error) = refused {
        return Err(error.to_string());
    }
    Ok(json!({
        "item_id": item_id,
        "dependencies": dependencies_json(&lineage::dependencies_of(&env.stack()?)),
    }))
}

pub(super) fn dependencies_json(edges: &[lineage::DependencyEdge]) -> Value {
    Value::Array(
        edges
            .iter()
            .map(|edge| json!({ "from": edge.from, "to": edge.to }))
            .collect(),
    )
}

/// `work_queue_graph`'s answer: each item's line edge and the stored dependency edges.
pub(super) fn graph_json(graph: &lineage::QueueGraph, unavailable_reason: Option<&str>) -> Value {
    let mut answer = json!({
        "line_edges": graph.line.iter().map(|(item_id, endpoint)| json!({
            "item_id": item_id, "kind": endpoint.kind(), "endpoint": endpoint.name(),
        })).collect::<Vec<_>>(),
        "dependencies": dependencies_json(&graph.dependencies),
    });
    if let Some(reason) = unavailable_reason {
        answer["unavailable_reason"] = json!(reason);
    }
    answer
}

/// Apply a link `edit` to the queue under its record lock; a refused edit writes nothing and is the
/// error, and the answer is the item's links as stored.
fn edit_links(
    env: &OpEnv,
    item_id: &str,
    edit: impl FnOnce(&mut WorkQueue) -> Result<(), item_links::ItemLinkError>,
) -> Result<Value, String> {
    let mut refused = None;
    env.home()?
        .update_stack(|queue| {
            let mut edited = queue.clone();
            match edit(&mut edited) {
                Ok(()) => *queue = edited,
                Err(error) => refused = Some(error),
            }
        })
        .map_err(|e| format!("link: {e}"))?;
    if let Some(error) = refused {
        return Err(error.to_string());
    }
    let stack = env.stack()?;
    Ok(json!({
        "item_id": item_id,
        "pr": item_links::assigned_pr_of(&stack, item_id).map(|pr| json!({
            "number": pr.number, "url": pr.url, "head_branch": pr.head_branch,
        })),
        "linked_sessions": item_links::linked_sessions_of(&stack, item_id)
            .into_iter()
            .map(|s| s.session_id)
            .collect::<Vec<_>>(),
    }))
}

/// Answer the node summaries of the stack as stored — read after the statuses were refreshed, so the
/// answer reflects what is stored. A refresh failure is surfaced as `refresh_error`, never hidden.
pub(super) fn status_answer(env: &OpEnv, refresh_error: Option<String>) -> Result<Value, String> {
    // Read after the possible persist, so the answer reflects what is stored.
    let stack = env.stack()?;
    let nodes: Vec<Value> = stack
        .nodes
        .iter()
        .map(|n| {
            json!({
                "node_id": n.node_id,
                "title": n.title,
                "branch": n.branch,
                "session_id": n.session_id,
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

/// Edit a node's metadata and, when asked, push the same title/body to its pull request.
///
/// Everything a `sync_pr` push can refuse without touching the network is checked *before* the
/// local edit, so a refused call leaves the plan exactly as it was. Only a failure GitHub itself
/// returns can leave the node edited and its pull request not, and that is reported as such.
fn update_planned(
    env: &OpEnv,
    github: &GithubSource,
    node_id: String,
    title: Option<String>,
    description: Option<String>,
    branch_suggestion: Option<String>,
    sync_pr: bool,
) -> Result<Value, String> {
    let home = env.home()?;
    // Resolved *before* the local edit: each of these can refuse without touching the network, and
    // a refusal that has already rewritten the plan is a partial application of a call the caller
    // was told had failed.
    let sync_client = if sync_pr {
        if title.is_none() && description.is_none() {
            return Err(format!(
                "node '{node_id}' was asked to sync its pull request, but neither a title nor a \
                 description was given — GitHub refuses an empty edit, so there is nothing to \
                 push and nothing was changed"
            ));
        }
        env.pull_number(&PrAddress::Node(node_id.clone()))?;
        Some(github.client()?)
    } else {
        None
    };

    let node = update_work_item(
        &home,
        UpdateWorkItemInput {
            node_id: node_id.clone(),
            title: title.clone(),
            description: description.clone(),
            branch_suggestion,
        },
    )?;
    let mut out = json!({
        "node_id": node.node_id,
        "title": node.title,
        "description": node.description,
        "branch_suggestion": node.branch_suggestion,
        "branch": node.branch,
        "session_id": node.session_id,
        "parents": node.parents,
    });
    if let Some(gh) = sync_client {
        // The preconditions held, so a failure here is GitHub itself refusing the write — the plan
        // is already edited and the pull request is not, which the caller has to be told.
        let number = sync_node_to_github_pr(
            &home,
            &node_id,
            title.as_deref(),
            description.as_deref(),
            &*gh,
        )
        .map_err(|e| format!("{e} (the node was updated; its pull request was not)"))?;
        out["pr_synced"] = json!(number);
    }
    Ok(out)
}

/// Record a manual internal-status override on a node, which derivation never overwrites.
fn set_status(
    env: &OpEnv,
    node_id: &str,
    kind: String,
    note: Option<String>,
) -> Result<Value, String> {
    let new_status = PrInternalStatus {
        kind,
        note,
        source: "override".to_string(),
    };
    let mut found = false;
    env.home()?
        .update_stack(|stack| {
            if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
                found = true;
                node.internal_status = Some(reconcile_internal_status(
                    node.internal_status.as_ref(),
                    new_status.clone(),
                ));
            }
        })
        .map_err(|e| format!("failed to write stack: {e}"))?;
    if found {
        Ok(json!({ "ok": true }))
    } else {
        Err(format!("node '{node_id}' not found"))
    }
}
