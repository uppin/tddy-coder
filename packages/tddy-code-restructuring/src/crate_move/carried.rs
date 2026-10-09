//! What a cross-crate move carries below a module: its directory children.
//!
//! A Rust 2018 module `a` is `a.rs`, and the files of its children sit in `a/`. A move of `a`
//! that renames `a.rs` alone strands them (`E0583`), so the move carries every file
//! [`files_of`](super::module_files::files_of) finds below the module, each to the same place relative to it under
//! the destination's `src/`. This module holds what the set of carried files decides on its own:
//! which plan members are already carried by another, which children cannot be carried, the note
//! naming what moved, and the refusal for a restricted child something outside the tree reaches.

use crate::plan::RefactorOp;
use crate::registry::Workspace;

use super::{ModuleHome, Move, MovingCluster, PlannedRewrite, Result};

/// The members of a cluster with every member a co-moving member carries taken out.
///
/// A member whose module path lies under another member's (`a::b` beside `a`) is one of that
/// member's directory children: it moves with it, at its nested position, so it is neither
/// renamed to the destination's root nor declared there, and its parent's `mod b;` line stays.
/// Order is kept for the members that remain.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-children): implement — `cluster::named_by` folds its members"
)]
pub(crate) fn fold_carried_members(members: Vec<ModuleHome>) -> Vec<ModuleHome> {
    // TODO(reshape-move-children): implement
    let _ = members;
    todo!("fold_carried_members")
}

/// Every reason operation `index` of `ops` (`op`) cannot carry one of its modules' children,
/// read statically, so a plain `check` reports what `apply` would refuse.
///
/// One finding per child, naming its file: a `mod x;` that leads to neither `x.rs` nor
/// `x/mod.rs`; a `mod` placed with `#[path]`; a target already present in the destination; a child
/// another operation of the plan moves on its own; a child's body reaching a module that stays
/// behind.
///
/// # Errors
///
/// Refuses when a file the check must read cannot be.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-children): implement — `preconditions::unrunnable` reports them"
)]
pub(crate) fn uncarriable(
    workspace: &Workspace<'_>,
    ops: &[RefactorOp],
    index: usize,
    op: &RefactorOp,
) -> Result<Vec<String>> {
    // TODO(reshape-move-children): implement
    let _ = (workspace, ops, index, op);
    todo!("uncarriable")
}

/// One note per member that carries more than its own file:
/// "`{n}` file(s) move with `{module}`, with the directory of its children".
///
/// # Errors
///
/// Refuses when a member's files cannot be read.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-children): implement — `crate_move::cluster_resolution` reports it"
)]
pub(crate) fn carried_notes(
    workspace: &Workspace<'_>,
    cluster: &MovingCluster,
) -> Result<Vec<String>> {
    // TODO(reshape-move-children): implement
    let _ = (workspace, cluster);
    todo!("carried_notes")
}

/// Refuse a move whose callers outside the carried tree reach a child the tree declares
/// `pub(crate)`, `pub(super)`, `pub(in …)` or private, through the child's own path: after the
/// move that path is private to the destination (`E0603`). The refusal names the child and every
/// referring file. Widening the child instead is not this operation's to do.
///
/// # Errors
///
/// The refusal, and anything reading the carried files refuses.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-children): implement — `resolve_cluster` calls it per member"
)]
pub(crate) fn restricted_children_reached(
    workspace: &Workspace<'_>,
    member: &Move,
    rewrites: &[PlannedRewrite],
) -> Result<()> {
    // TODO(reshape-move-children): implement
    let _ = (workspace, member, rewrites);
    todo!("restricted_children_reached")
}
