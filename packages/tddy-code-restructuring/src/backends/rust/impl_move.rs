//! `move_impl_members`: moving a run of members of one inherent `impl T` into an `impl T` block of
//! another module of the same crate.
//!
//! rust-analyzer has no assist for this, and `extract_module`'s inherent-member path can only write
//! a new child module of the file and widens every private member to `pub(crate)`. Like
//! `move_item`, this operation is authored here and informed by the server: where the members are
//! comes from the outline, who calls them from `textDocument/references`, and everything else is a
//! function of the files' text, in [`assemble`]. No caller is edited — a method resolves through its
//! type wherever its block is written. See `docs/dev/1-WIP/2026-10-09-reshape-move-impl-members.md`
//! for the rules (R1–R7) and the refusals (P1–P9, S1–S6).

#[allow(
    dead_code,
    reason = "TODO(reshape-move-impl-members): the survey (milestone 4) is the first caller of this module"
)]
pub(super) mod assemble;
#[allow(
    dead_code,
    reason = "TODO(reshape-move-impl-members): the survey (milestone 4) is the first caller of this module"
)]
pub(super) mod landing;

use super::item_move::creation;
use super::item_move::destination::{find_module, package_of, Lookup};
use super::item_move::preflight::{anchored, does_not_exist, Named};
use super::{failure, RustBackend};
use crate::edit::Resolution;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

/// The refusal every entry point makes until the operation is implemented.
fn not_implemented_yet() -> crate::RestructureError {
    crate::RestructureError::UnsupportedOp {
        backend: "`move_impl_members` is not implemented yet (#reshape 12/24)".to_string(),
        op: "MoveImplMembers".to_string(),
    }
}

/// What a static check finds wrong with a `move_impl_members`, from the plan and the text alone:
/// P2 (the destination is in another crate), then P3–P5 (the destination, or the parent of the
/// module it creates, and whether the members already live there).
pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    let origin = origin_module(op)?;
    let package = package_of(workspace.root, op.anchor.file())?;
    let to = op.to.as_deref().unwrap_or_default();
    let mut pieces = to.split("::").map(str::trim);
    let crate_of_to = pieces.next().unwrap_or_default().replace('-', "_");
    if crate_of_to != package.crate_name {
        return Ok(vec![format!(
            "`{to}` is in `{crate_of_to}`, and the members are in `{}`: an inherent `impl` outside \
             the type's crate is `E0116`",
            package.crate_name
        )]);
    }
    let named = Named {
        package,
        to: to.to_string(),
        module: pieces.map(str::to_string).collect(),
        creates: op.name.clone(),
    };
    destination_findings(workspace, &named, &origin)
}

/// The module the members are declared in, below the crate root.
fn origin_module(op: &RefactorOp) -> Result<Vec<String>> {
    anchored(op)
        .map(|(module, _)| module)
        .ok_or_else(|| failure("`move_impl_members` anchors by item (`items`, or a single `item`)"))
}

/// P3–P5 for a destination read against its package: a module that does not exist, a parent that
/// does not exist or already declares the name, and a destination that is the origin module.
fn destination_findings(
    workspace: &Workspace<'_>,
    named: &Named,
    origin: &[String],
) -> Result<Vec<String>> {
    let parent = match find_module(workspace, &named.package, &named.module)? {
        Lookup::Found(module) => module,
        Lookup::Missing { at } => return Ok(vec![does_not_exist(named, at)]),
    };
    match &named.creates {
        Some(name) => Ok(creation::within(workspace, named, name, parent)?.0),
        None if parent.path == origin => Ok(vec![format!(
            "the members are already in `{}`: the destination is the module they are declared in",
            named.to
        )]),
        None => Ok(Vec::new()),
    }
}

impl RustBackend {
    /// Resolve a `move_impl_members` into the edits to every file it changes.
    // TODO(reshape-move-impl-members): milestone 4 wires the survey (R6), the landing and the
    // assembly here; until then the operation is refused rather than run without its widenings.
    pub(super) fn move_impl_members(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        let _ = (op, workspace);
        Err(not_implemented_yet())
    }
}
