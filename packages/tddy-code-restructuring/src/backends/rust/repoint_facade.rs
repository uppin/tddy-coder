//! `repoint_facade_imports`: naming what a file uses by the crate that defines it.
//!
//! A module that is to move into another crate must name what it uses by its defining crate, or
//! the move presents an edge back to the crate it leaves. This operation applies the path survey's
//! answer (`crate_move::survey`) to one file or one module and writes nothing else: no facade is
//! removed, no manifest is edited, and no server is asked anything. See
//! `docs/dev/1-WIP/2026-10-05-sharpen-repoint-facade.md` for the rules.
//!
//! TODO(repoint-facade): the operation is published and not implemented. Until it is, `resolve`
//! refuses naming the node, so nothing believes a path was re-pointed. `check` finds nothing to
//! report from the text alone, on purpose: a static finding stops `check --deep` from rehearsing
//! (`check_plan` skips `resolve` for an operation with one), and the rehearsal is where the refusal
//! is reported and, once implemented, where the list of paths is printed.

mod group;
mod refusals;
mod rewrite;
mod scope;

use super::RustBackend;
use crate::edit::Resolution;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

/// What a static check finds wrong with a `repoint_facade_imports`, from the plan and the text of
/// the file a `symbol` anchor names alone: the refusals `resolve` gives for paths it cannot
/// rewrite. A module anchor needs `--deep` (it must be lowered), and says so.
///
/// TODO(repoint-facade): implement; see [`refusals`]. Nothing is reported until then, so
/// `check --deep` still reaches the refusal `resolve` raises.
pub(super) fn findings(_op: &RefactorOp, _workspace: &Workspace<'_>) -> Result<Vec<String>> {
    Ok(Vec::new())
}

impl RustBackend {
    /// Resolve a `repoint_facade_imports` into the edits to the files its anchor names, and the
    /// notes listing every path rewritten (`file:line: written -> defined`).
    ///
    /// TODO(repoint-facade): implement `scope::files_of`, `rewrite::path_edits` and
    /// `group::split_or_reprefix`, and build the edits and notes from them.
    pub(super) fn repoint_facade_imports(
        &mut self,
        _op: &RefactorOp,
        _workspace: &Workspace<'_>,
    ) -> Result<Resolution> {
        Err(refusals::unfinished())
    }
}
