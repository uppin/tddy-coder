//! Putting a member move together: the new text of every file it touches.
//!
//! A function of the original texts and of what the server said — the run of members (from the
//! origin's outline), the root items of the origin the moved members name, and the member widenings
//! `item_move::members::widen_members` computed — so the whole assembly is read, and tested, without
//! a server. The members' bytes are copied from the origin; only a visibility (R6) or a relative
//! spelling (R5) inside them is edited, and the only text authored is a new block's header line,
//! its braces, and the `use` lines (R4).

use std::collections::BTreeMap;

use super::super::item_move::destination::Module;
use super::super::item_move::members::MemberWidening;
use super::super::item_move::outline::Item;
use super::super::retarget_impl::outline::Run;
use crate::edit::VisibilityChange;
use crate::Result;

/// What a member move is made of.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-impl-members): built by `move_impl_members` once it is implemented"
)]
pub(in crate::backends::rust) struct MovingMembers<'a> {
    pub(in crate::backends::rust) source_file: &'a str,
    pub(in crate::backends::rust) source_text: &'a str,
    /// The origin module, below the crate root.
    pub(in crate::backends::rust) source: &'a [String],
    /// The block the members leave and which of its members move.
    pub(in crate::backends::rust) run: &'a Run,
    /// Where the members land. For a created module its file is not on disk yet and its scope is
    /// empty.
    pub(in crate::backends::rust) destination: &'a Module,
    /// The destination file's text; empty for a created module.
    pub(in crate::backends::rust) destination_text: &'a str,
    /// The parent that declares the destination, and its name, when the move creates it.
    pub(in crate::backends::rust) created: Option<(&'a Module, &'a str)>,
    /// The root items of the origin the moved members name (the self type among them when the
    /// origin defines it): each gets a `use` in the destination.
    pub(in crate::backends::rust) reached: &'a [Item],
    /// The visibility edits (in origin coordinates) and report lines of R6.
    pub(in crate::backends::rust) widening: &'a MemberWidening,
}

/// The result of a member move: the new text of each file it changes.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-impl-members): read by `move_impl_members` once it is implemented"
)]
pub(in crate::backends::rust) struct Assembled {
    pub(in crate::backends::rust) files: BTreeMap<String, (String, String)>,
    pub(in crate::backends::rust) report: Vec<VisibilityChange>,
    pub(in crate::backends::rust) notes: Vec<String>,
}

#[allow(
    dead_code,
    reason = "TODO(reshape-move-impl-members): called by `move_impl_members` once it is implemented"
)]
pub(in crate::backends::rust) fn assemble(moving: &MovingMembers<'_>) -> Result<Assembled> {
    // TODO(reshape-move-impl-members): implement R2–R5 (the cut, the emptied block, the landing,
    // the imports, the relative spellings) over `super::landing::block_for`.
    let _ = moving;
    Err(crate::RestructureError::UnsupportedOp {
        backend: "`move_impl_members` is not implemented yet (#reshape 13/19)".to_string(),
        op: "MoveImplMembers".to_string(),
    })
}

#[cfg(test)]
mod assemble_tests;
