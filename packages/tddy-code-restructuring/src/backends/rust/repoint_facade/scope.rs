//! Which files an anchor names: one file for a `symbol` anchor, every file of the module for an
//! anchor lowered from an `items` anchor on a `mod` declaration.
//!
//! TODO(repoint-facade): implement; unused until `resolve` calls it.

#![allow(dead_code)]

use super::refusals::unfinished;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

/// The files the operation acts on, relative to the workspace root, in a stable order.
///
/// TODO(repoint-facade): `Anchor::Symbol` names `file`; a lowered module anchor names
/// `module_files::files_of` of the module's file, inline modules followed.
pub(super) fn files_of(_op: &RefactorOp, _workspace: &Workspace<'_>) -> Result<Vec<String>> {
    Err(unfinished())
}
