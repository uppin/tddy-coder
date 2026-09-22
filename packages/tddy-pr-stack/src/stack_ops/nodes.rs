//! Planned-node writers: add, update, delete, and the plan-level parent rewrite.

use std::path::Path;

use tddy_core::changeset::StackNode;

use super::order::assign_missing_display_order;
use super::repoint::realign_node_to_effective_base;
use super::{
    append_node_atomic, next_free_node_id, read_stack, reject_if_cyclic, validate_parents,
};

/// Input for [`add_planned_pr_node`]. A struct rather than positional params since several
/// fields share the same `Option<String>` shape — grouping them removes the transposition risk.
pub struct AddPlannedPrInput {
    pub title: String,
    pub description: String,
    pub branch_suggestion: Option<String>,
    pub parents: Vec<String>,
    /// Accepted for symmetry with the stack plan's `PlannedPr` (recipe-side, in
    /// `tddy-workflow-recipes`) but currently unused: like the plan's
    /// `planned_prs_into_stack_nodes`, `StackNode` has no `child_recipe`
    /// field to carry it — the web client defaults to `"tdd"` at start-session time regardless
    /// (see `PrStackScreen.tsx`'s `handleStartSession`).
    pub child_recipe: Option<String>,
}

/// Append one manually-created planned PR to an orchestrator session's persisted stack,
/// choosing its ancestors (parent node ids) from the already-planned nodes.
///
/// Unlike `tddy_workflow_recipes::pr_stack::reseed_stack_from_plan_if_unspawned` (agent-driven,
/// replaces the whole plan wholesale and refuses once any node has spawned), this appends a single
/// node and never touches existing nodes — safe to call regardless of how many nodes have already
/// spawned child sessions.
///
/// The new node's `node_id` is always server-assigned (see `next_free_node_id`) — callers
/// never supply one. Rejects (without writing) a `parents` entry that doesn't resolve to an
/// existing node, or an append that would introduce a cycle.
///
/// PRD: `docs/ft/coder/pr-stacking.md` § Manually adding a planned PR.
pub fn add_planned_pr_node(
    session_dir: &Path,
    input: AddPlannedPrInput,
) -> Result<StackNode, String> {
    const OP: &str = "add_planned_pr_node";

    let existing = read_stack(session_dir, OP)?;

    // No `op` prefix: this rejection shipped bare and is read verbatim by its callers.
    validate_parents(&existing, None, &input.parents, "")?;

    let node_id = next_free_node_id(&existing);
    let new_node = StackNode {
        node_id,
        title: input.title,
        description: input.description,
        // A suggestion is a planned name, not a ref: `branch` stays empty until a child worktree
        // actually creates it (same contract as [`planned_prs_into_stack_nodes`]).
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
    };

    // Defense-in-depth cycle check: parents are restricted to pre-existing node ids above, so an
    // append alone can never actually cycle, but this keeps the same guard `validate_stack_plan`
    // applies to a whole plan, cheaply, rather than special-casing the append path as exempt.
    let mut candidate_nodes = existing.nodes.clone();
    candidate_nodes.push(new_node.clone());
    reject_if_cyclic(
        &tddy_core::changeset::Stack {
            version: existing.version,
            nodes: candidate_nodes,
        },
        "",
    )?;

    append_node_atomic(session_dir, new_node, OP)
}

/// Which of a node's metadata fields an update rewrites. A field left `None` is untouched, which is
/// how a caller edits a title without having to restate the description.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdatePlannedPrInput {
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
    pub node: StackNode,
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
/// Never touches `parents` (see [`set_stack_node_parents`]), `branch`, `session_id`, `pr_status` or
/// `internal_status`, and never contacts GitHub — pushing an edit to the PR is
/// [`sync_node_to_github_pr`], asked for separately.
///
/// [`sync_node_to_github_pr`]: super::sync_node_to_github_pr
pub fn update_planned_pr_node(
    session_dir: &Path,
    input: UpdatePlannedPrInput,
) -> Result<StackNode, String> {
    use tddy_core::changeset::{read_changeset, update_stack_atomic};

    let node_id = input.node_id.clone();
    if input.title.is_none() && input.description.is_none() && input.branch_suggestion.is_none() {
        return Err(format!(
            "update_planned_pr_node: the update of node '{node_id}' names no field to change \
             (expected at least one of title, description, branch_suggestion)"
        ));
    }

    let stack = read_changeset(session_dir)
        .map_err(|e| format!("update_planned_pr_node: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    let node = stack
        .node(&node_id)
        .ok_or_else(|| format!("update_planned_pr_node: node '{node_id}' not found"))?;
    if let (Some(branch), Some(suggestion)) =
        (node.branch.as_deref(), input.branch_suggestion.as_deref())
    {
        return Err(format!(
            "update_planned_pr_node: node '{node_id}' already owns branch '{branch}', so its \
             branch_suggestion cannot be rewritten to '{suggestion}'"
        ));
    }

    let mut updated: Option<StackNode> = None;
    update_stack_atomic(session_dir, |stack| {
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
    .map_err(|e| format!("update_planned_pr_node: failed to write stack: {e}"))?;

    // `update_stack_atomic` re-reads the file before applying the edit, so a node the check above saw
    // can still be gone by then — a concurrent writer removed it. Reporting that is not the same as
    // reporting a successful edit.
    updated.ok_or_else(|| {
        format!("update_planned_pr_node: node '{node_id}' vanished before the update was written")
    })
}

/// Remove a node from the stack, reparenting its children onto that node's own parents.
///
/// Reparenting is what keeps the DAG whole. [`tddy_core::changeset::Stack::topo_order`] counts
/// in-degree only over parents that resolve to a node, so a parent id pointing at a removed node is
/// silently ignored by every existing check — a delete that simply dropped the node would leave the
/// stack quietly describing an edge that no longer exists. Children therefore inherit the removed
/// node's parents; a child that already lists one of them does not gain a duplicate. Deleting a root
/// leaves its children as roots, based off the stack bottom.
///
/// Refuses a node whose PR is **open**. Closing a PR is externally visible and is the agent's to ask
/// for explicitly via `pr_close`; a node whose PR is merged, closed, errored or absent deletes
/// freely.
///
/// The node's branch, worktree and child session are left untouched and reported — see
/// [`DeletedNode`].
pub fn delete_planned_pr_node(session_dir: &Path, node_id: &str) -> Result<DeletedNode, String> {
    use tddy_core::changeset::{read_changeset, update_stack_atomic};

    let stack = read_changeset(session_dir)
        .map_err(|e| format!("delete_planned_pr_node: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    let node = stack
        .node(node_id)
        .ok_or_else(|| format!("delete_planned_pr_node: node '{node_id}' not found"))?;
    if node
        .pr_status
        .as_ref()
        .is_some_and(|status| status.phase == "open")
    {
        return Err(format!(
            "delete_planned_pr_node: node '{node_id}' has an open pull request — merge it with \
             pr_merge or close it with pr_close first"
        ));
    }

    // Validate the stack the delete *would* produce before writing anything. `topo_order` ignores
    // parent ids that resolve to no node, so a delete is the one mutation that could leave the stack
    // describing an edge nothing checks — the reparenting below is what keeps it whole.
    let mut candidate = stack.clone();
    remove_node_reparenting_children(&mut candidate, node_id);
    reject_if_cyclic(&candidate, "delete_planned_pr_node")?;

    let mut removed: Option<(StackNode, Vec<String>)> = None;
    update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        removed = remove_node_reparenting_children(stack, node_id);
    })
    .map_err(|e| format!("delete_planned_pr_node: failed to write stack: {e}"))?;

    let (node, reparented_children) = removed.ok_or_else(|| {
        format!("delete_planned_pr_node: node '{node_id}' vanished before the delete was written")
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
    stack: &mut tddy_core::changeset::Stack,
    node_id: &str,
) -> Option<(StackNode, Vec<String>)> {
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
/// This is the plan-level move, distinct from [`repoint_planned_pr_node`]: repointing answers "the
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
/// [`repoint_planned_pr_node`]: super::repoint_planned_pr_node
pub fn set_stack_node_parents(
    session_dir: &Path,
    repo_root: &Path,
    node_id: &str,
    parents: &[String],
    default_branch: &str,
    gh: &dyn tddy_github::pr_api::GithubPrApi,
) -> Result<StackNode, String> {
    use tddy_core::changeset::{read_changeset, update_stack_atomic};
    const OP: &str = "set_stack_node_parents";

    let stack = read_changeset(session_dir)
        .map_err(|e| format!("{OP}: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    if stack.node(node_id).is_none() {
        return Err(format!("{OP}: node '{node_id}' not found"));
    }

    validate_parents(&stack, Some(node_id), parents, OP)?;

    // Same guard `add_planned_pr_node` applies to an append: unlike an append, an arbitrary parent
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

/// Re-read the node a move just wrote, so [`set_stack_node_parents`] decides whether to realign from
/// the stack on disk rather than from its own pre-write snapshot.
///
/// `update_stack_atomic` re-reads before applying its closure, and another writer (`pr_spawn_child`,
/// or the web's start-session path through `link_stack_node_to_child_session`) can bind a branch to
/// this node in between. Gating on the snapshot's value would persist the new parents and silently
/// skip the rebase, the force-push and the PR re-target — while reporting success.
fn reload_moved_node(session_dir: &Path, node_id: &str) -> Result<StackNode, String> {
    const OP: &str = "set_stack_node_parents";
    tddy_core::changeset::read_changeset(session_dir)
        .map_err(|e| format!("{OP}: failed to reload node: {e}"))?
        .stack
        .unwrap_or_default()
        .node(node_id)
        .cloned()
        .ok_or_else(|| format!("{OP}: node '{node_id}' vanished after the move"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack_ops::test_support::{
        a_changeset_with_stack, a_node, a_started_node, assert_rejected, node_ids, stack_on_disk,
        write_stack,
    };
    use rstest::rstest;
    use std::path::Path;
    use tddy_core::changeset::{read_changeset, Changeset, StackNode};

    // -----------------------------------------------------------------------
    // add_planned_pr_node
    // -----------------------------------------------------------------------

    #[test]
    fn appending_a_root_planned_pr_to_an_empty_stack_assigns_n1_and_persists_it() {
        // Given — a session with no stack yet
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        tddy_core::changeset::write_changeset(dir, &Changeset::default()).unwrap();

        // When
        let result = add_planned_pr_node(
            dir,
            AddPlannedPrInput {
                title: "Add token store".to_string(),
                description: "Persists refresh tokens.".to_string(),
                branch_suggestion: Some("feature/token-store".to_string()),
                parents: vec![],
                child_recipe: None,
            },
        );

        // Then
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        let node = result.unwrap();
        assert_eq!(node.node_id, "n1");
        assert_eq!(node.title, "Add token store");
        assert_eq!(node.description, "Persists refresh tokens.");
        assert_eq!(
            node.branch_suggestion.as_deref(),
            Some("feature/token-store")
        );
        assert_eq!(node.parents, Vec::<String>::new());

        let loaded = read_changeset(dir).unwrap().stack.unwrap();
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.node("n1").unwrap().title, "Add token store");
    }

    #[test]
    fn appending_a_node_with_valid_parents_persists_them_and_assigns_the_next_free_id() {
        // Given — a stack with two existing nodes, n1 and n2
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = a_changeset_with_stack(vec![
            a_node("n1", "Add token store", vec![]),
            a_node("n2", "Add auth middleware", vec!["n1"]),
        ]);
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When — the new node depends on both existing nodes
        let result = add_planned_pr_node(
            dir,
            AddPlannedPrInput {
                title: "Add token refresh endpoint".to_string(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec!["n1".to_string(), "n2".to_string()],
                child_recipe: None,
            },
        );

        // Then
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        let node = result.unwrap();
        assert_eq!(node.node_id, "n3");
        assert_eq!(node.parents, vec!["n1".to_string(), "n2".to_string()]);

        let loaded = read_changeset(dir).unwrap().stack.unwrap();
        assert_eq!(loaded.nodes.len(), 3);
        assert_eq!(
            loaded.node("n3").unwrap().parents,
            vec!["n1".to_string(), "n2".to_string()]
        );
    }

    #[test]
    fn a_dangling_parent_ref_is_rejected_and_the_stack_on_disk_is_unchanged() {
        // Given — a stack with a single node, n1
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = a_changeset_with_stack(vec![a_node("n1", "Add token store", vec![])]);
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When — the requested ancestor "n99" does not exist
        let result = add_planned_pr_node(
            dir,
            AddPlannedPrInput {
                title: "Add auth middleware".to_string(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec!["n99".to_string()],
                child_recipe: None,
            },
        );

        // Then
        assert!(result.is_err(), "expected Err for a dangling parent ref");
        assert!(result.unwrap_err().contains("n99"));
        let loaded = read_changeset(dir).unwrap().stack.unwrap();
        assert_eq!(loaded.nodes.len(), 1, "stack on disk must be unchanged");
    }

    #[test]
    fn the_new_node_always_stays_planned_with_no_session_id_or_pr_status() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        tddy_core::changeset::write_changeset(dir, &Changeset::default()).unwrap();

        // When
        let node = add_planned_pr_node(
            dir,
            AddPlannedPrInput {
                title: "Add token store".to_string(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec![],
                child_recipe: None,
            },
        )
        .unwrap();

        // Then
        assert_eq!(node.session_id, None);
        assert_eq!(node.pr_status, None);
        assert_eq!(node.branch, None);
        assert_eq!(node.child_state, None);
    }

    #[test]
    fn node_id_assignment_picks_up_after_a_non_contiguous_max() {
        // Given — a stack whose highest existing node id is "n5", not the node count (2)
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let cs = a_changeset_with_stack(vec![
            a_node("n1", "Add token store", vec![]),
            a_node("n5", "Add auth middleware", vec![]),
        ]);
        tddy_core::changeset::write_changeset(dir, &cs).unwrap();

        // When
        let node = add_planned_pr_node(
            dir,
            AddPlannedPrInput {
                title: "Add token refresh endpoint".to_string(),
                description: String::new(),
                branch_suggestion: None,
                parents: vec![],
                child_recipe: None,
            },
        )
        .unwrap();

        // Then — next id is one past the max ("n6"), not one past the count ("n3")
        assert_eq!(node.node_id, "n6");
    }

    fn parents_of(dir: &Path, node_id: &str) -> Vec<String> {
        stack_on_disk(dir).node(node_id).unwrap().parents.clone()
    }

    fn an_update_of(node_id: &str) -> UpdatePlannedPrInput {
        UpdatePlannedPrInput {
            node_id: node_id.to_string(),
            ..UpdatePlannedPrInput::default()
        }
    }

    // -----------------------------------------------------------------------
    // update_planned_pr_node
    // -----------------------------------------------------------------------

    #[test]
    fn updating_a_nodes_title_and_description_persists_both_and_leaves_every_other_field_untouched()
    {
        // Given — n1 is fully started: it owns a branch, a child session and an open PR
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![a_started_node("n1", "feature/n1", "open", vec![])],
        );
        let before = stack_on_disk(dir).node("n1").unwrap().clone();

        // When — the operator clarifies what the PR is for
        let node = update_planned_pr_node(
            dir,
            UpdatePlannedPrInput {
                title: Some("Token store, split out".to_string()),
                description: Some("Extracted from the parent PR.".to_string()),
                ..an_update_of("n1")
            },
        )
        .expect("editing intent on a started node should succeed");

        // Then — only the two fields named changed; the node's identity and reality did not
        assert_eq!(node.title, "Token store, split out");
        assert_eq!(node.description, "Extracted from the parent PR.");
        assert_eq!(node.branch, before.branch);
        assert_eq!(node.session_id, before.session_id);
        assert_eq!(node.pr_status, before.pr_status);
        assert_eq!(node.parents, before.parents);
        assert_eq!(
            stack_on_disk(dir).node("n1").unwrap().title,
            "Token store, split out"
        );
    }

    #[test]
    fn an_update_that_names_no_field_is_rejected_and_the_stack_on_disk_is_unchanged() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![a_node("n1", "Original title", vec![])]);

        // When — every field left unset
        let result = update_planned_pr_node(dir, an_update_of("n1"));

        // Then — a no-op update can only be a caller mistake, so it is reported rather than hidden
        assert_rejected(result).with_reason_containing("names no field to change");
        assert_eq!(
            stack_on_disk(dir).node("n1").unwrap().title,
            "Original title"
        );
    }

    #[test]
    fn a_branch_suggestion_edit_is_accepted_while_the_node_is_still_planned() {
        // Given — n1 was never started, so its suggestion is still the only name it has
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![StackNode {
                branch_suggestion: Some("feature/old-name".to_string()),
                ..a_node("n1", "Add token store", vec![])
            }],
        );

        // When
        let node = update_planned_pr_node(
            dir,
            UpdatePlannedPrInput {
                branch_suggestion: Some("feature/token-store".to_string()),
                ..an_update_of("n1")
            },
        )
        .expect("renaming a planned node's suggested branch should succeed");

        // Then — read back from disk as well as returned: an edit applied to a clone and never
        // persisted would be no edit at all
        assert_eq!(
            node.branch_suggestion.as_deref(),
            Some("feature/token-store")
        );
        assert_eq!(
            stack_on_disk(dir)
                .node("n1")
                .unwrap()
                .branch_suggestion
                .as_deref(),
            Some("feature/token-store")
        );
    }

    #[test]
    fn a_branch_suggestion_edit_is_rejected_once_the_node_owns_a_branch() {
        // Given — a worktree already exists, so the suggestion has been superseded by a real ref
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![StackNode {
                branch_suggestion: Some("feature/suggested".to_string()),
                ..a_started_node("n1", "feature/actual", "open", vec![])
            }],
        );

        // When
        let result = update_planned_pr_node(
            dir,
            UpdatePlannedPrInput {
                branch_suggestion: Some("feature/renamed".to_string()),
                ..an_update_of("n1")
            },
        );

        // Then — refused, so the plan never claims a branch name nothing uses
        assert_rejected(result).with_reason_containing("feature/actual");
        assert_eq!(
            stack_on_disk(dir)
                .node("n1")
                .unwrap()
                .branch_suggestion
                .as_deref(),
            Some("feature/suggested")
        );
    }

    #[test]
    fn updating_an_unknown_node_id_is_rejected() {
        // Given — a stack with no node called n9
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![a_node("n1", "Add token store", vec![])]);

        // When
        let result = update_planned_pr_node(
            dir,
            UpdatePlannedPrInput {
                title: Some("Retitled".to_string()),
                ..an_update_of("n9")
            },
        );

        // Then
        assert_rejected(result).with_reason_containing("n9");
    }

    // -----------------------------------------------------------------------
    // delete_planned_pr_node
    // -----------------------------------------------------------------------

    #[test]
    fn deleting_a_middle_node_reparents_its_children_onto_that_nodes_parents() {
        // Given — a chain n1 → n2 → n3 where n2 turned out to be unnecessary
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![
                a_node("n1", "one", vec![]),
                a_node("n2", "two", vec!["n1"]),
                a_node("n3", "three", vec!["n2"]),
            ],
        );

        // When
        let deleted =
            delete_planned_pr_node(dir, "n2").expect("deleting a planned node should succeed");

        // Then — n3 inherits n1, so the chain stays connected
        assert_eq!(deleted.node.node_id, "n2");
        assert_eq!(deleted.reparented_children, vec!["n3".to_string()]);
        assert_eq!(node_ids(dir), vec!["n1", "n3"]);
        assert_eq!(parents_of(dir, "n3"), vec!["n1".to_string()]);
    }

    #[test]
    fn deleting_a_root_node_leaves_its_children_as_roots() {
        // Given — n1 is a root and n2 sits on it
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![a_node("n1", "one", vec![]), a_node("n2", "two", vec!["n1"])],
        );

        // When
        delete_planned_pr_node(dir, "n1").expect("deleting a root should succeed");

        // Then — n2 becomes a root itself, basing off the stack bottom
        assert_eq!(node_ids(dir), vec!["n2"]);
        assert_eq!(parents_of(dir, "n2"), Vec::<String>::new());
    }

    #[test]
    fn a_child_that_already_lists_the_inherited_parent_does_not_gain_a_duplicate() {
        // Given — a diamond: n3 depends on both n1 and n2, and n2 itself depends on n1
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![
                a_node("n1", "one", vec![]),
                a_node("n2", "two", vec!["n1"]),
                a_node("n3", "three", vec!["n1", "n2"]),
            ],
        );

        // When — n2 is removed, so n3 would inherit n1 it already has
        delete_planned_pr_node(dir, "n2").expect("deleting a diamond's middle should succeed");

        // Then — one edge to n1, not two
        assert_eq!(parents_of(dir, "n3"), vec!["n1".to_string()]);
    }

    #[test]
    fn no_reference_to_the_deleted_node_survives_in_any_nodes_parents() {
        // Given — two separate children both depend on n2
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![
                a_node("n1", "one", vec![]),
                a_node("n2", "two", vec!["n1"]),
                a_node("n3", "three", vec!["n2"]),
                a_node("n4", "four", vec!["n2"]),
            ],
        );

        // When
        delete_planned_pr_node(dir, "n2").expect("deleting should succeed");

        // Then — a dangling parent ref would be silently ignored by `topo_order`, so none may remain
        let dangling: Vec<String> = stack_on_disk(dir)
            .nodes
            .iter()
            .filter(|n| n.parents.iter().any(|p| p == "n2"))
            .map(|n| n.node_id.clone())
            .collect();
        assert_eq!(dangling, Vec::<String>::new());
        assert_eq!(parents_of(dir, "n3"), vec!["n1".to_string()]);
        assert_eq!(parents_of(dir, "n4"), vec!["n1".to_string()]);
    }

    #[test]
    fn deleting_a_node_whose_pr_is_open_is_rejected_and_the_stack_on_disk_is_unchanged() {
        // Given — n1's PR is open on GitHub
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![a_started_node("n1", "feature/n1", "open", vec![])],
        );

        // When
        let result = delete_planned_pr_node(dir, "n1");

        // Then — closing a PR is externally visible and must be asked for explicitly
        assert_rejected(result).with_reason_containing("open");
        assert_eq!(node_ids(dir), vec!["n1"]);
    }

    #[rstest]
    #[case::merged("merged")]
    #[case::closed("closed")]
    #[case::errored("error")]
    fn deleting_a_node_whose_pr_is_no_longer_open_is_allowed(#[case] phase: &str) {
        // Given — a node whose PR has already left the open state
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![a_started_node("n1", "feature/n1", phase, vec![])]);

        // When
        delete_planned_pr_node(dir, "n1").expect("a node with no open PR should delete");

        // Then
        assert_eq!(node_ids(dir), Vec::<String>::new());
    }

    #[test]
    fn deleting_a_started_node_reports_its_orphaned_branch_and_session_id() {
        // Given — n1 owns a branch and a child session, and its PR is already merged
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(
            dir,
            vec![a_started_node("n1", "feature/n1", "merged", vec![])],
        );

        // When
        let deleted = delete_planned_pr_node(dir, "n1").expect("deleting should succeed");

        // Then — deletion is a plan operation, so what it left unowned is named, not removed
        assert_eq!(deleted.orphaned_branch.as_deref(), Some("feature/n1"));
        assert_eq!(
            deleted.orphaned_session_id.as_deref(),
            Some("session-for-n1")
        );
    }

    #[test]
    fn deleting_an_unknown_node_id_is_rejected() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_stack(dir, vec![a_node("n1", "one", vec![])]);

        // When
        let result = delete_planned_pr_node(dir, "n9");

        // Then
        assert_rejected(result).with_reason_containing("n9");
        assert_eq!(node_ids(dir), vec!["n1"]);
    }
}
