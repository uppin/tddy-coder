//! The PR-stack operations: every writer of a session's `Changeset.stack`, and the git/GitHub
//! syncs that keep a node's branch and pull request in line with it.
//!
//! Moved out of `tddy-workflow-recipes`' `pr_stack` module, which still re-exports all of it from
//! its historical path; the `pr-stack` recipe itself, its hooks, and the plan→stack bridge
//! (`reseed_stack_from_plan_if_unspawned`) stay there.

//!
//! Split by concern; every public item is re-exported here, so `tddy_pr_stack::stack_ops::*` is the
//! one path callers name.

use std::path::Path;

use tddy_core::changeset::StackNode;

mod adopt;
mod nodes;
mod order;
mod pull_base;
mod repoint;
mod seed;

pub use adopt::{
    adopt_pr_as_stack_node, adopt_pr_into_stack, sync_node_to_github_pr, AdoptedPrFacts,
};
pub use nodes::{
    add_planned_pr_node, delete_planned_pr_node, set_stack_node_parents, update_planned_pr_node,
    AddPlannedPrInput, DeletedNode, UpdatePlannedPrInput,
};
pub use order::{assign_missing_display_order, move_planned_pr_node};
pub use pull_base::{pull_base_into_node_branch, BaseSyncStrategy, PullBaseReport};
pub use repoint::repoint_planned_pr_node;
pub use seed::{
    check_stack_seed_base, check_stack_seed_not_self, seed_stack_with_base_session, SeededBase,
    StackSeedBaseRefusal,
};

use order::next_display_order;

/// The session's persisted stack, or an empty one when its changeset records none.
///
/// `op` prefixes the failure so the caller's name reaches the operator — every writer below reports
/// an unreadable changeset the same way.
fn read_stack(dir: &Path, op: &str) -> Result<tddy_core::changeset::Stack, String> {
    Ok(tddy_core::changeset::read_changeset(dir)
        .map_err(|e| format!("{op}: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default())
}

/// Append one node to the persisted stack, numbering it last in the operator's reading order, and
/// answer the node as it was written.
///
/// Every append shares this, rather than spelling it out per writer, because the two statements in the
/// closure are a **stack invariant** and not this caller's business: [`assign_missing_display_order`]
/// numbers whatever the stack on disk left unnumbered, and only then is
/// [`next_display_order`] the position this node takes. A writer that forgot the first statement would
/// hand the new node a position an older node already holds, and the panel would order the two
/// arbitrarily.
///
/// Both decisions are made **inside** the `update_stack_atomic` closure, against the stack that is
/// about to be written: `update_stack_atomic` re-reads the file before applying its closure, so a
/// position chosen from a snapshot taken earlier could collide with a node another writer appended in
/// between.
fn append_node_atomic(dir: &Path, node: StackNode, op: &str) -> Result<StackNode, String> {
    let mut appended = node;
    tddy_core::changeset::update_stack_atomic(dir, |stack| {
        assign_missing_display_order(stack);
        appended.display_order = Some(next_display_order(stack));
        stack.nodes.push(appended.clone());
    })
    .map_err(|e| format!("{op}: failed to write stack: {e}"))?;
    Ok(appended)
}

/// Reject a parent list before anything is written: an id that resolves to no node, and — when
/// `node_id` names an *existing* node being rewritten — that node naming itself, or the same parent
/// named twice.
///
/// `node_id` is `None` for an append. The node does not exist yet, so it has no id for a parent to
/// collide with, and a repeated id is not rejected there — that is [`add_planned_pr_node`]'s and
/// [`adopt_pr_as_stack_node`]'s shipped behaviour, kept as it is.
///
/// `op` prefixes every message it emits, **except** that an empty `op` emits them bare. That
/// asymmetry is deliberate, not drift: [`add_planned_pr_node`] shipped `dangling parent ref: n9`
/// unprefixed and its callers read that exact string, so prefixing it now would change behaviour
/// already in use.
fn validate_parents(
    stack: &tddy_core::changeset::Stack,
    node_id: Option<&str>,
    parents: &[String],
    op: &str,
) -> Result<(), String> {
    let prefix = message_prefix(op);
    let mut seen: Vec<&String> = Vec::with_capacity(parents.len());
    for parent in parents {
        if node_id == Some(parent.as_str()) {
            return Err(format!("{prefix}node '{parent}' cannot be its own parent"));
        }
        if !stack.nodes.iter().any(|n| &n.node_id == parent) {
            return Err(format!("{prefix}dangling parent ref: {parent}"));
        }
        if node_id.is_some() && seen.contains(&parent) {
            return Err(format!("{prefix}parent '{parent}' is named more than once"));
        }
        seen.push(parent);
    }
    Ok(())
}

/// Reject the stack a mutation *would* produce when it is no longer a DAG.
///
/// Every writer here validates a candidate before touching the file, so a rejected call leaves the
/// stack on disk exactly as it was. `op` follows the same empty-means-unprefixed rule as
/// [`validate_parents`], for the same reason.
fn reject_if_cyclic(candidate: &tddy_core::changeset::Stack, op: &str) -> Result<(), String> {
    candidate
        .topo_order()
        .map_err(|e| format!("{}cycle detected: {e}", message_prefix(op)))?;
    Ok(())
}

/// `"{op}: "`, or `""` for an empty `op` — see [`validate_parents`] for why one writer emits its
/// rejections unprefixed.
fn message_prefix(op: &str) -> String {
    if op.is_empty() {
        String::new()
    } else {
        format!("{op}: ")
    }
}

/// Next free `"n<N>"` node id for a stack: one past the highest existing numeric suffix among
/// ids matching `n<digits>` (non-matching ids are ignored for this purpose), or `"n1"` for an
/// empty/all-non-matching stack. Uses the max, not the count, so a stack with a gap (e.g. `"n1"`,
/// `"n5"`) still assigns `"n6"` rather than colliding.
fn next_free_node_id(stack: &tddy_core::changeset::Stack) -> String {
    let max = stack
        .nodes
        .iter()
        .filter_map(|n| n.node_id.strip_prefix('n')?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("n{}", max + 1)
}

/// Fixtures shared by the submodules' unit tests.
#[cfg(test)]
mod test_support {
    use std::path::Path;
    use tddy_core::changeset::{read_changeset, Changeset, GithubPrStatus, Stack, StackNode};

    pub(super) fn a_changeset_with_stack(nodes: Vec<StackNode>) -> Changeset {
        Changeset {
            stack: Some(Stack { version: 1, nodes }),
            ..Changeset::default()
        }
    }

    pub(super) fn a_node(node_id: &str, title: &str, parents: Vec<&str>) -> StackNode {
        StackNode {
            node_id: node_id.to_string(),
            title: title.to_string(),
            description: String::new(),
            branch_suggestion: None,
            branch: None,
            session_id: None,
            parents: parents.into_iter().map(str::to_string).collect(),
            pr_status: None,
            child_state: None,
            internal_status: None,
            display_order: None,
        }
    }

    // -----------------------------------------------------------------------
    // shared fixtures for the full-control primitives
    //
    // PRD: docs/ft/coder/pr-stacking.md § Full control over the plan.
    // Changeset: docs/dev/changesets/2026-07-30-pr-stack-full-control.md.
    // -----------------------------------------------------------------------

    /// [`a_node`] plus the things a started node owns: a branch, a child session, and a PR recorded
    /// at `phase`.
    pub(super) fn a_started_node(
        node_id: &str,
        branch: &str,
        phase: &str,
        parents: Vec<&str>,
    ) -> StackNode {
        StackNode {
            branch: Some(branch.to_string()),
            session_id: Some(format!("session-for-{node_id}")),
            pr_status: Some(GithubPrStatus {
                phase: phase.to_string(),
                url: Some(format!("https://github.com/acme/repo/pull/{node_id}")),
                error: None,
            }),
            ..a_node(node_id, node_id, parents)
        }
    }

    pub(super) fn write_stack(dir: &Path, nodes: Vec<StackNode>) {
        tddy_core::changeset::write_changeset(dir, &a_changeset_with_stack(nodes)).unwrap();
    }

    pub(super) fn stack_on_disk(dir: &Path) -> Stack {
        read_changeset(dir).unwrap().stack.unwrap()
    }

    pub(super) fn node_ids(dir: &Path) -> Vec<String> {
        stack_on_disk(dir)
            .nodes
            .iter()
            .map(|n| n.node_id.clone())
            .collect()
    }

    /// A rejected call, so a test reads `assert_rejected(r).with_reason_containing("n9")`.
    pub(super) struct Rejection(String);

    pub(super) fn assert_rejected<T: std::fmt::Debug>(result: Result<T, String>) -> Rejection {
        match result {
            Err(reason) => Rejection(reason),
            Ok(value) => {
                panic!("expected the call to be rejected, but it succeeded with {value:?}")
            }
        }
    }

    impl Rejection {
        pub(super) fn with_reason_containing(self, fragment: &str) -> Self {
            assert!(
                self.0.contains(fragment),
                "expected the rejection to mention '{fragment}', was '{}'",
                self.0
            );
            self
        }
    }
}
