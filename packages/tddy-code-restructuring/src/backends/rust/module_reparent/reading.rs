//! What a `reparent_module` plan says, read from the plan and the anchor alone.

use super::super::item_move::preflight::{anchored, named_by, Named};
use super::super::item_move::text::{line_end, line_start};
use super::super::{failure, is_identifier};
use crate::crate_move::source_scan::items_of_module;
use crate::item_anchor::{module_path_of, unlowered_item_anchor};
use crate::plan::{Anchor, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

/// The module to move and where it goes.
pub(super) struct Reparent {
    /// The new parent: its package, its path as written in the plan and below the crate root.
    pub(super) named: Named,
    /// The file the anchor is in, which should be the old parent's.
    pub(super) file: String,
    /// The old parent, below the crate root.
    pub(super) parent: Vec<String>,
    /// The module's own name.
    pub(super) name: String,
}

impl Reparent {
    /// The module as the crate root sees it: the old parent and its name.
    pub(super) fn module_path(&self) -> Vec<String> {
        let mut path = self.parent.clone();
        path.push(self.name.clone());
        path
    }
}

/// The plan's operation, read against the package the anchor's file belongs to.
pub(super) fn read(workspace: &Workspace<'_>, op: &RefactorOp) -> Result<Reparent> {
    let named = named_by(workspace, op, "reparent_module")?;
    let (parent, names) = match anchored(op) {
        Some(read) => read,
        None => written_in(workspace, op)?,
    };
    let [name] = names.as_slice() else {
        return Err(failure(format!(
            "`reparent_module` moves one module per operation, and the anchor names {}",
            names.len()
        )));
    };
    Ok(Reparent {
        named,
        file: op.anchor.file().to_string(),
        parent,
        name: name.clone(),
    })
}

/// The module and the declared names a lowered anchor holds, read from the lines it covers.
fn written_in(workspace: &Workspace<'_>, op: &RefactorOp) -> Result<(Vec<String>, Vec<String>)> {
    let Anchor::Range { file, start, end } = &op.anchor else {
        return Err(unlowered_item_anchor(&op.anchor, "`reparent_module`"));
    };
    let text = workspace.read(file)?;
    let region = line_start(&text, start.line)..line_end(&text, end.line);
    let parent = module_path_of(workspace.root, file)?.split_off(1);
    let names = items_of_module(&text[region])
        .children
        .into_iter()
        .map(|child| child.name)
        .filter(|name| is_identifier(name))
        .collect();
    Ok((parent, names))
}
