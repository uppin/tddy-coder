//! A crate a cross-crate move creates, because its plan line names one with `name`.
//!
//! Scaffolding a crate used to be refused as authoring rather than moving. The facts a skeleton
//! needs are all in the repository, though: the package name is the plan's, the `version` and
//! `edition` lines are the origin's, copied as written, and the `members` entry is the one a move
//! into an unlisted crate already writes. So `move_module_to_crate` and `move_cluster_to_crate`
//! create the crate when — and only when — their line carries `name`, in the same edit as the move.
//!
//! The skeleton is resolved into first, so every edit the move makes to the new manifest and root
//! reads them as text that exists, and is then folded into one creation and one change per file.

use crate::crate_move::destination::Destination;
use crate::edit::FileEdit;
use crate::overlay::Overlay;
use crate::plan::RefactorOp;
use crate::registry::Workspace;

use super::Result;

/// The crate a move creates: where, and under which `[package] name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCrate {
    /// The new crate's directory, relative to the repository root — the plan's `to`.
    pub dir: String,
    /// Its `[package] name` — the plan's `name`.
    pub package: String,
}

impl NewCrate {
    /// The crate `op` creates, when it is a module crate move whose line carries `name`.
    #[allow(
        dead_code,
        reason = "TODO(reshape-new-crate): implement — `Move::read` and `cluster::named_by` read it"
    )]
    pub(crate) fn named_by(op: &RefactorOp) -> Option<NewCrate> {
        // TODO(reshape-new-crate): implement
        let _ = op;
        todo!("NewCrate::named_by")
    }

    /// The destination this crate will be, without reading a manifest that does not exist yet.
    #[allow(
        dead_code,
        reason = "TODO(reshape-new-crate): implement — `Move::read` uses it when the op creates"
    )]
    pub(crate) fn destination(&self) -> Destination {
        Destination {
            dir: self.dir.clone(),
            package: self.package.clone(),
            extern_name: self.package.replace('-', "_"),
        }
    }

    /// Why this crate cannot be created in `workspace`, moving code out of `origin`, if it cannot:
    /// a crate already at `dir`, a directory that is not empty, an extern name the origin or a
    /// listed member already has, or a workspace with no `members` array.
    #[allow(
        dead_code,
        reason = "TODO(reshape-new-crate): implement — `move_preconditions` reports it"
    )]
    pub(crate) fn refusal(
        &self,
        workspace: &Workspace<'_>,
        origin: &Destination,
    ) -> Result<Option<String>> {
        // TODO(reshape-new-crate): implement
        let _ = (workspace, origin);
        todo!("NewCrate::refusal")
    }

    /// The skeleton the move resolves into: `[package]` with this name and the origin's `version`
    /// and `edition` lines verbatim, and an empty `src/lib.rs`.
    #[allow(
        dead_code,
        reason = "TODO(reshape-new-crate): implement — `cluster::widened_cluster` seeds it"
    )]
    pub(crate) fn skeleton(
        &self,
        workspace: &Workspace<'_>,
        origin: &Destination,
    ) -> Result<Skeleton> {
        // TODO(reshape-new-crate): implement
        let _ = (workspace, origin);
        todo!("NewCrate::skeleton")
    }
}

/// The two files a new crate starts as, before the move fills them.
#[allow(
    dead_code,
    reason = "TODO(reshape-new-crate): implement — built by `NewCrate::skeleton`, read by `folded`"
)]
pub(crate) struct Skeleton {
    /// `<dir>/Cargo.toml` as created.
    pub(crate) manifest: String,
    /// `<dir>/src/lib.rs` as created.
    pub(crate) root: String,
    /// The two created paths, manifest first.
    pub(crate) created: [String; 2],
}

/// The move's `changes` with every edit to a created file folded into its skeleton text: one
/// `FileEdit::Create` and one `FileEdit::Change` per created file, so no path gets two changes.
#[allow(
    dead_code,
    reason = "TODO(reshape-new-crate): implement — `cluster::widened_cluster` folds its edit"
)]
pub(crate) fn folded(skeleton: &Skeleton, changes: Vec<FileEdit>) -> Result<Vec<FileEdit>> {
    // TODO(reshape-new-crate): implement
    let _ = (skeleton, changes);
    todo!("new_crate::folded")
}

/// An overlay over `workspace` holding the skeleton of every crate an operation before `index`
/// creates, so a plain `check` reads operation `index` at its place in the plan.
#[allow(
    dead_code,
    reason = "TODO(reshape-new-crate): implement — `preconditions::unrunnable` seeds it"
)]
pub(crate) fn created_by_earlier_operations(
    workspace: &Workspace<'_>,
    ops: &[RefactorOp],
    index: usize,
) -> Result<Overlay> {
    // TODO(reshape-new-crate): implement
    let _ = (workspace, ops, index);
    todo!("new_crate::created_by_earlier_operations")
}
