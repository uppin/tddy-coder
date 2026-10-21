//! `detach_method`: turning the members of one inherent `impl` block into free functions over a
//! named parameter, and re-pointing every caller the language server reports.
//!
//! The rules are the changeset's R1–R6 (`#reshape` 15/24, `feature/reshape/detach-method`):
//!
//! - **R1 signature** — `&mut self` → `name: &mut T`, `&self` → `name: &T`, the receiver's
//!   lifetime kept; an associated function keeps its parameters; doc comments, attributes and
//!   visibility unchanged; the function is written in the member's own module, so it reaches exactly
//!   the callers the method did and nothing is widened.
//! - **R2 body** — `self` → `name` (`read_fields_through`'s self-mode rewrite, reused), `Self` → the
//!   block header's self type.
//! - **R3 placement** — the functions follow the block in member order; a block the run empties is
//!   replaced by them.
//! - **R4 callers** — `R.m(args)` → `m(R, args)` (a place receiver borrowed `&mut R` / `&R`, a
//!   reference passed as it is, the arguments byte for byte); `Self::m` / `T::m` → `m`; from another
//!   module `M::m(…)` plus a `use` of `M` when the file does not bind it; a comment is not edited.
//! - **R5 refusals** — made by [`check`] before any edit is built, every offending member or site
//!   at once, each with its file and line.
//! - **R6 note** — `detach_method: T::a, T::b became functions over `session`; N calls in M files
//!   re-pointed (K `use` added); the block was emptied and removed`.
//!
//! Everything the operation asks the server goes through [`DetachServer`], whose answers are the
//! language server's own shapes. [`RustBackend`] answers it from rust-analyzer (`server`); a library
//! test answers it from recorded answers, so the rules are decided without a server.

mod block;
mod build;
mod calls;
mod member;
mod refusals;
mod scan;
mod server;
mod splice;
mod survey;

use serde_json::Value;

use super::seam_refusal;
use crate::crate_move::Reference;
use crate::edit::{Position, Range, Resolution};
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

/// What `detach_method` asks the language server, in the shapes the server answers in.
pub trait DetachServer {
    /// The lines the operation's anchor covers: an `items` or `item` anchor lowered the way the
    /// runner lowers one, or a range anchor as written.
    fn span(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Range>;

    /// The settled `textDocument/documentSymbol` answer for `file` (relative to the workspace
    /// root): the `impl` blocks (kind 19) with their members as children.
    fn outline(&mut self, workspace: &Workspace<'_>, file: &str) -> Result<Value>;

    /// Every place the server reports naming the member whose name is written at `at` (one-based)
    /// in `file`, its declaration excluded.
    fn references(
        &mut self,
        workspace: &Workspace<'_>,
        file: &str,
        at: Position,
    ) -> Result<Vec<Reference>>;

    /// The server's `textDocument/hover` answer at `at` (one-based) in `file`: how the receiver of
    /// a call is typed.
    fn hover(&mut self, workspace: &Workspace<'_>, file: &str, at: Position) -> Result<Value>;
}

/// What a plain `check` finds wrong with a `detach_method` from the anchored file's text alone,
/// without a server. The plan-line refusals (a missing or malformed `name`, an anchor of the wrong
/// kind, a field the operation does not define) are made earlier, when the plan is read.
pub fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
    survey::plain_findings(op, workspace)
}

/// The R5 refusals of a `detach_method`, as `check --deep` reports them: every member that cannot
/// be detached and every call site that cannot be rewritten, each naming its file and line. Empty
/// when the operation can run.
pub fn check(
    server: &mut dyn DetachServer,
    op: &RefactorOp,
    workspace: &Workspace<'_>,
) -> Result<Vec<String>> {
    Ok(survey::survey(server, op, workspace)?.findings)
}

/// Resolve a `detach_method` into the edits to the file that holds the block and to every file that
/// calls one of its members, with the R6 note; refused with every [`check`] finding when there is
/// one, before any edit is built.
pub fn resolve(
    server: &mut dyn DetachServer,
    op: &RefactorOp,
    workspace: &Workspace<'_>,
) -> Result<Resolution> {
    let surveyed = survey::survey(server, op, workspace)?;
    if !surveyed.findings.is_empty() {
        return Err(seam_refusal(surveyed.findings.join("\n")));
    }
    build::resolution(surveyed)
}
