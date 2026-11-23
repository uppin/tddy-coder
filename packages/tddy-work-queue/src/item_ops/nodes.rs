//! Planned-node writers: add, update, delete, and the plan-level parent rewrite.

use std::path::Path;

use tddy_core::changeset::WorkItem;

use super::order::assign_missing_display_order;
use super::repoint::realign_node_to_effective_base;
use super::{
    append_node_atomic, next_free_node_id, read_stack, reject_if_cyclic, validate_parents,
    WorkQueueHome,
};

/// Input for [`add_work_item`]. A struct rather than positional params since several
/// fields share the same `Option<String>` shape — grouping them removes the transposition risk.
pub struct AddWorkItemInput {
    pub title: String,
    pub description: String,
    pub branch_suggestion: Option<String>,
    pub parents: Vec<String>,
    /// Accepted but currently unused: `WorkItem` has no `child_recipe` field to carry it — the web
    /// client defaults to `"tdd"` at start-session time regardless.
    pub child_recipe: Option<String>,
}

/// Append one manually-created planned PR to an orchestrator session's persisted stack,
/// choosing its ancestors (parent node ids) from the already-planned nodes.
///
/// This appends a single node and never touches existing nodes — safe to call regardless of how
/// many nodes have already spawned child sessions.
///
/// The new node's `node_id` is always server-assigned (see `next_free_node_id`) — callers
/// never supply one. Rejects (without writing) a `parents` entry that doesn't resolve to an
/// existing node, or an append that would introduce a cycle.
///
/// PRD: `docs/ft/coder/pr-stacking.md` § Manually adding a planned PR.
pub fn add_work_item(
    session_dir: &(impl WorkQueueHome + ?Sized),
    input: AddWorkItemInput,
) -> Result<WorkItem, String> {
    const OP: &str = "add_work_item";

    let existing = read_stack(session_dir, OP)?;

    // No `op` prefix: this rejection shipped bare and is read verbatim by its callers.
    validate_parents(&existing, None, &input.parents, "")?;

    let node_id = next_free_node_id(&existing);
    let new_node = WorkItem {
        node_id,
        title: input.title,
        description: input.description,
        // A suggestion is a planned name, not a ref: `branch` stays empty until a child worktree
        // actually creates it.
        branch: None,
        branch_suggestion: input.branch_suggestion,
        session_id: None,
        parents: input.parents,
        pr_status: None,
        child_state: None,
        internal_status: None,
        // Chosen by `append_node_atomic` below, against the stack that is actually about to be
        // written.
        display_order: None,
        ..Default::default()
    };

    // Defense-in-depth cycle check: parents are restricted to pre-existing node ids above, so an
    // append alone can never actually cycle, but this keeps the same guard every other writer of the
    // stack applies, cheaply, rather than special-casing the append path as exempt.
    let mut candidate_nodes = existing.nodes.clone();
    candidate_nodes.push(new_node.clone());
    reject_if_cyclic(
        &tddy_core::changeset::WorkQueue {
            version: existing.version,
            nodes: candidate_nodes,
            ..Default::default()
        },
        "",
    )?;

    append_node_atomic(session_dir, new_node, OP)
}

/// Which of a node's metadata fields an update rewrites. A field left `None` is untouched, which is
/// how a caller edits a title without having to restate the description.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateWorkItemInput {
    pub node_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub branch_suggestion: Option<String>,
}

/// What a deletion removed, and what it left behind.
///
/// `orphaned_branch` and `orphaned_session_id` are reported rather than cleaned up: deletion is a
/// *plan* operation, and silently deleting a branch or a child session would destroy work the
/// operator never asked to lose. Naming them lets the agent tell the operator what is now unowned.
#[derive(Debug, Clone, PartialEq)]
pub struct DeletedNode {
    pub node: WorkItem,
    /// Ids of the children that inherited the removed node's parents.
    pub reparented_children: Vec<String>,
    pub orphaned_branch: Option<String>,
    pub orphaned_session_id: Option<String>,
}

/// Rewrite a node's `title`, `description` and/or `branch_suggestion`.
///
/// `title` and `description` are editable at any point in a node's life, including once it owns a
/// branch, a child session and an open PR — they are the plan's description of intent, and intent
/// gets clarified. `branch_suggestion` is not: once `branch` is set the suggestion has been
/// superseded by a real ref, and rewriting it would leave the plan claiming a name nothing uses.
///
/// An input naming no field at all is rejected rather than treated as a successful no-op: it can
/// only be a caller mistake, and reporting success would hide it.
///
/// Never touches `parents` (see [`set_work_item_parents`]), `branch`, `session_id`, `pr_status` or
/// `internal_status`, and never contacts GitHub — pushing an edit to the PR is
/// [`sync_node_to_github_pr`], asked for separately.
///
/// [`sync_node_to_github_pr`]: super::sync_node_to_github_pr
pub fn update_work_item(
    session_dir: &(impl WorkQueueHome + ?Sized),
    input: UpdateWorkItemInput,
) -> Result<WorkItem, String> {
    let node_id = input.node_id.clone();
    if input.title.is_none() && input.description.is_none() && input.branch_suggestion.is_none() {
        return Err(format!(
            "update_work_item: the update of node '{node_id}' names no field to change \
             (expected at least one of title, description, branch_suggestion)"
        ));
    }

    let stack = session_dir
        .read_stack()
        .map_err(|e| format!("update_work_item: failed to read changeset: {e}"))?
        .unwrap_or_default();
    let node = stack
        .node(&node_id)
        .ok_or_else(|| format!("update_work_item: node '{node_id}' not found"))?;
    if let (Some(branch), Some(suggestion)) =
        (node.branch.as_deref(), input.branch_suggestion.as_deref())
    {
        return Err(format!(
            "update_work_item: node '{node_id}' already owns branch '{branch}', so its \
             branch_suggestion cannot be rewritten to '{suggestion}'"
        ));
    }

    let mut updated: Option<WorkItem> = None;
    session_dir
        .update_stack(|stack| {
            assign_missing_display_order(stack);
            if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
                if let Some(title) = input.title {
                    node.title = title;
                }
                if let Some(description) = input.description {
                    node.description = description;
                }
                if let Some(branch_suggestion) = input.branch_suggestion {
                    node.branch_suggestion = Some(branch_suggestion);
                }
                updated = Some(node.clone());
            }
        })
        .map_err(|e| format!("update_work_item: failed to write stack: {e}"))?;

    // `update_stack_atomic` re-reads the file before applying the edit, so a node the check above saw
    // can still be gone by then — a concurrent writer removed it. Reporting that is not the same as
    // reporting a successful edit.
    updated.ok_or_else(|| {
        format!("update_work_item: node '{node_id}' vanished before the update was written")
    })
}

/// Remove a node from the stack, reparenting its children onto that node's own parents.
///
/// Reparenting is what keeps the DAG whole. [`tddy_core::changeset::WorkQueue::topo_order`] counts
/// in-degree only over parents that resolve to a node, so a parent id pointing at a removed node is
/// silently ignored by every existing check — a delete that simply dropped the node would leave the
/// stack quietly describing an edge that no longer exists. Children therefore inherit the removed
/// node's parents; a child that already lists one of them does not gain a duplicate. Deleting a root
/// leaves its children as roots, based off the stack bottom.
///
/// Refuses a node whose PR is **open**. Closing a PR is externally visible and is the agent's to ask
/// for explicitly via `work_item_close`; a node whose PR is merged, closed, errored or absent deletes
/// freely.
///
/// The node's branch, worktree and child session are left untouched and reported — see
/// [`DeletedNode`].
pub fn delete_work_item(
    session_dir: &(impl WorkQueueHome + ?Sized),
    node_id: &str,
) -> Result<DeletedNode, String> {
    let stack = session_dir
        .read_stack()
        .map_err(|e| format!("delete_work_item: failed to read changeset: {e}"))?
        .unwrap_or_default();
    let node = stack
        .node(node_id)
        .ok_or_else(|| format!("delete_work_item: node '{node_id}' not found"))?;
    if node
        .pr_status
        .as_ref()
        .is_some_and(|status| status.phase == "open")
    {
        return Err(format!(
            "delete_work_item: node '{node_id}' has an open pull request — merge it with \
             work_item_merge or close it with work_item_close first"
        ));
    }

    // Validate the stack the delete *would* produce before writing anything. `topo_order` ignores
    // parent ids that resolve to no node, so a delete is the one mutation that could leave the stack
    // describing an edge nothing checks — the reparenting below is what keeps it whole.
    let mut candidate = stack.clone();
    remove_node_reparenting_children(&mut candidate, node_id);
    reject_if_cyclic(&candidate, "delete_work_item")?;

    let mut removed: Option<(WorkItem, Vec<String>)> = None;
    session_dir
        .update_stack(|stack| {
            assign_missing_display_order(stack);
            removed = remove_node_reparenting_children(stack, node_id);
        })
        .map_err(|e| format!("delete_work_item: failed to write stack: {e}"))?;

    let (node, reparented_children) = removed.ok_or_else(|| {
        format!("delete_work_item: node '{node_id}' vanished before the delete was written")
    })?;
    Ok(DeletedNode {
        orphaned_branch: node.branch.clone(),
        orphaned_session_id: node.session_id.clone(),
        node,
        reparented_children,
    })
}

/// Remove `node_id` from `stack`, giving every child that listed it the removed node's parents
/// instead. Returns the removed node and the ids of the children that inherited, in stack order, or
/// `None` when the stack holds no such node.
///
/// A pure function of the stack so the same rule decides the candidate that is validated and the
/// write that is applied to the freshly-read stack — `update_stack_atomic` re-reads before applying
/// its closure, so a result computed from an earlier snapshot could describe a stack that was never
/// written.
///
/// A child that already lists an inherited parent keeps its single edge: `parents` is a set of
/// ancestors, and the same ancestor twice would make the node look like a two-parent merge.
fn remove_node_reparenting_children(
    stack: &mut tddy_core::changeset::WorkQueue,
    node_id: &str,
) -> Option<(WorkItem, Vec<String>)> {
    let position = stack.nodes.iter().position(|n| n.node_id == node_id)?;
    let removed = stack.nodes.remove(position);

    // Nothing here writes a node that lists itself as its own parent, but a stack on disk is written
    // by several processes and read back unvalidated. Inheriting such an entry verbatim would hand
    // every child a reference to the node just removed — and `topo_order` ignores parent ids that
    // resolve to no node, so nothing downstream would ever report it. Dropping it makes "no dangling
    // reference survives a delete" hold whatever the stack said.
    let inherited_parents: Vec<String> = removed
        .parents
        .iter()
        .filter(|parent| parent.as_str() != node_id)
        .cloned()
        .collect();

    let mut reparented = Vec::new();
    for child in &mut stack.nodes {
        if !child.parents.iter().any(|parent| parent == node_id) {
            continue;
        }
        let mut parents: Vec<String> = Vec::with_capacity(child.parents.len());
        for parent in &child.parents {
            let inherited: &[String] = if parent == node_id {
                &inherited_parents
            } else {
                std::slice::from_ref(parent)
            };
            for id in inherited {
                if !parents.contains(id) {
                    parents.push(id.clone());
                }
            }
        }
        child.parents = parents;
        reparented.push(child.node_id.clone());
    }
    Some((removed, reparented))
}

/// Set a node's parents outright, then bring git and GitHub in line with the new position.
///
/// This is the plan-level move, distinct from [`repoint_work_item`]: repointing answers "the
/// base branch drifted, retain the parent that owns this target", whereas this answers "the plan
/// changed, this node belongs *here* now". `parents` is the complete new set — an empty list makes
/// the node a root, based off the stack bottom.
///
/// Rejects an unknown parent id, a node naming itself, a repeated id, and any change that would
/// close a cycle. Nothing is written when validation fails, so a rejected call leaves the stack on
/// disk exactly as it was.
///
/// A node that owns no branch is a plan-only move: the persisted parent change is the whole effect.
/// A node that owns one is then realigned exactly as a repoint realigns it — rebased onto the new
/// effective base, force-pushed with lease, and its open PR re-targeted.
///
/// [`repoint_work_item`]: super::repoint_work_item
pub fn set_work_item_parents(
    session_dir: &Path,
    repo_root: &Path,
    node_id: &str,
    parents: &[String],
    default_branch: &str,
    gh: &dyn tddy_github::pr_api::GithubPrApi,
) -> Result<WorkItem, String> {
    use tddy_core::changeset::update_stack_atomic;
    const OP: &str = "set_work_item_parents";

    let stack = tddy_core::changeset::read_session_stack(session_dir)
        .map_err(|e| format!("{OP}: failed to read changeset: {e}"))?
        .unwrap_or_default();
    if stack.node(node_id).is_none() {
        return Err(format!("{OP}: node '{node_id}' not found"));
    }

    validate_parents(&stack, Some(node_id), parents, OP)?;

    // Same guard `add_work_item` applies to an append: unlike an append, an arbitrary parent
    // rewrite really can close a cycle, so this one is load-bearing rather than defensive.
    let mut candidate = stack.clone();
    if let Some(candidate_node) = candidate.nodes.iter_mut().find(|n| n.node_id == node_id) {
        candidate_node.parents = parents.to_vec();
    }
    reject_if_cyclic(&candidate, OP)?;

    update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.parents = parents.to_vec();
        }
    })
    .map_err(|e| format!("{OP}: failed to write stack: {e}"))?;

    let moved = reload_moved_node(session_dir, node_id)?;

    // A node that owns no branch is a plan-only move: there is nothing to rebase and no pull request
    // of its own to re-target, so the persisted parent change is the whole effect.
    if let Some(branch) = moved.branch.as_deref() {
        realign_node_to_effective_base(
            OP,
            session_dir,
            repo_root,
            node_id,
            branch,
            default_branch,
            gh,
        )?;
    }

    Ok(moved)
}

/// Re-read the node a move just wrote, so [`set_work_item_parents`] decides whether to realign from
/// the stack on disk rather than from its own pre-write snapshot.
///
/// `update_stack_atomic` re-reads before applying its closure, and another writer (`work_item_spawn_session`,
/// or the web's start-session path through `link_work_item_to_session`) can bind a branch to
/// this node in between. Gating on the snapshot's value would persist the new parents and silently
/// skip the rebase, the force-push and the PR re-target — while reporting success.
fn reload_moved_node(session_dir: &Path, node_id: &str) -> Result<WorkItem, String> {
    const OP: &str = "set_work_item_parents";
    tddy_core::changeset::read_session_stack(session_dir)
        .map_err(|e| format!("{OP}: failed to reload node: {e}"))?
        .unwrap_or_default()
        .node(node_id)
        .cloned()
        .ok_or_else(|| format!("{OP}: node '{node_id}' vanished after the move"))
}

#[cfg(test)]
mod tests;
