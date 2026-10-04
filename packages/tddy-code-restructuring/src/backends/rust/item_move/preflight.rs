//! What the plan and the text of the tree already say about a move, before any server exists.
//!
//! A destination that is not there (or, for a line that names a module to create, a parent that is
//! not there or already declares the name), a name it already declares and a destination that is the items'
//! own module are lexical facts. Reading them here is what lets `check` report them without paying
//! for an index, and what lets `apply` refuse a plan that cannot be honoured before it spawns one.

use std::collections::BTreeSet;

use super::super::{failure, is_identifier};
use super::creation::{self, Destination};
use super::destination::{find_module, package_of, Lookup, Package};
use super::written_in;
use crate::crate_move::source_scan::items_of_module;
use crate::plan::{Anchor, ItemPath, RefactorOp};
use crate::registry::Workspace;
use crate::Result;

/// The destination a plan names: the package it is in, and the module path below the crate root.
pub(in crate::backends::rust) struct Named {
    pub(in crate::backends::rust) package: Package,
    pub(in crate::backends::rust) to: String,
    pub(in crate::backends::rust) module: Vec<String>,
    /// The name of a module the move creates inside `module`, when the plan line carries `name`.
    pub(in crate::backends::rust) creates: Option<String>,
}

/// `to` read against the package the anchor's file belongs to.
///
/// Refused when it is absent or names another crate: a move stays inside one crate, and the plan
/// that wants more is a `move_module_to_crate`.
pub(in crate::backends::rust) fn named_by(
    workspace: &Workspace<'_>,
    op: &RefactorOp,
    operation: &str,
) -> Result<Named> {
    let to = op
        .to
        .as_deref()
        .ok_or_else(|| failure(format!("`{operation}` needs `to`: the module to move into")))?;
    let package = package_of(workspace.root, op.anchor.file())?;
    let mut pieces = to.split("::").map(str::trim);
    let crate_name = pieces.next().unwrap_or_default();
    if crate_name.replace('-', "_") != package.crate_name {
        return Err(failure(format!(
            "`{to}` is in `{crate_name}`, and the anchor is in `{}`: `{operation}` stays inside \
             one crate",
            package.crate_name
        )));
    }
    Ok(Named {
        module: pieces.map(str::to_string).collect(),
        package,
        to: to.to_string(),
        creates: op.name.clone(),
    })
}

/// The module of the items an anchor names, below the crate root, and their names.
///
/// `None` for an anchor that does not name items by path.
pub(in crate::backends::rust) fn anchored(op: &RefactorOp) -> Option<(Vec<String>, Vec<String>)> {
    let paths: &[ItemPath] = match &op.anchor {
        Anchor::Items { items, .. } => items,
        Anchor::Item { item, .. } => std::slice::from_ref(item),
        _ => return None,
    };
    let first = paths.first()?.pieces();
    let module = first.get(1..first.len().saturating_sub(1))?;
    let names = paths
        .iter()
        .filter_map(|path| path.pieces().last().copied())
        .filter(|name| is_identifier(name))
        .map(str::to_string)
        .collect();
    Some((
        module.iter().map(|piece| piece.to_string()).collect(),
        names,
    ))
}

/// Everything the text says is wrong with moving the items an anchor names to `to`.
///
/// Every finding rather than the first, because a plan is checked to be fixed in one pass.
pub(super) fn findings(workspace: &Workspace<'_>, op: &RefactorOp) -> Result<Vec<String>> {
    let named = named_by(workspace, op, "move_item")?;
    // A deep check hands over the plan with its item anchors already lowered to the lines they
    // cover, the same operation `apply` runs; those lines are read as `move_items` reads them. A
    // `range` or `symbol` anchor never reaches here from a plan, because the codec refuses it.
    let (source, names) = match anchored(op) {
        Some(read) => read,
        None => written_in(workspace, op, &workspace.read(op.anchor.file())?)?,
    };
    Ok(obstacles(workspace, &named, &source, &names)?.0)
}

/// The destination module, or the refusal for why the items cannot move into it.
pub(super) fn destination_for(
    workspace: &Workspace<'_>,
    named: &Named,
    source: &[String],
    names: &[String],
) -> Result<Destination> {
    match obstacles(workspace, named, source, names)? {
        (found, _) if !found.is_empty() => Err(failure(found.join("; "))),
        (_, Some(destination)) => Ok(destination),
        (_, None) => Err(failure(format!("`{}` does not exist", named.to))),
    }
}

/// The findings, and the destination when it exists (or, for a move that creates it, can be made).
fn obstacles(
    workspace: &Workspace<'_>,
    named: &Named,
    source: &[String],
    names: &[String],
) -> Result<(Vec<String>, Option<Destination>)> {
    let module = match find_module(workspace, &named.package, &named.module)? {
        Lookup::Found(module) => module,
        Lookup::Missing { at } => return Ok((vec![does_not_exist(named, at)], None)),
    };
    if let Some(name) = &named.creates {
        return creation::within(workspace, named, name, module);
    }

    if module.path == source {
        let subject = if names.is_empty() {
            "the items".to_string()
        } else {
            format!("`{}`", names.join("`, `"))
        };
        return Ok((
            vec![format!(
                "{subject} is already in `{}`: the destination is the module the items are \
                 declared in",
                named.to
            )],
            Some(Destination::existing(module)),
        ));
    }

    let text = workspace.read(&module.file)?;
    let taken = names_declared_in(&text[module.scope.clone()]);
    let clashes = names
        .iter()
        .filter(|name| taken.contains(name.as_str()))
        .map(|name| {
            format!(
                "`{name}` is already declared in `{}`, so moving a second one there would be \
                 `E0428`",
                named.to
            )
        })
        .collect();
    Ok((clashes, Some(Destination::existing(module))))
}

/// The finding for a destination whose path stops existing at segment `at` of the module path.
pub(in crate::backends::rust) fn does_not_exist(named: &Named, at: usize) -> String {
    let parent = std::iter::once(named.package.crate_name.as_str())
        .chain(named.module[..at].iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("::");
    let missing = format!(
        "`{}` does not exist: `{parent}` declares no module `{}`",
        named.to, named.module[at]
    );
    if named.creates.is_some() {
        return missing;
    }
    format!(
        "{missing}. A move creates a module only when its line carries a `name`: `to` is then the \
         parent the new module is declared in"
    )
}

/// The names a module's own text binds at its top level: what it defines, the modules it declares
/// and what its `use` items bring into scope.
pub(in crate::backends::rust) fn names_declared_in(module_text: &str) -> BTreeSet<String> {
    let items = items_of_module(module_text);
    let mut names: BTreeSet<String> = items.defined.into_iter().collect();
    names.extend(items.children.into_iter().map(|child| child.name));
    names.extend(
        items
            .uses
            .iter()
            .filter_map(|leaf| leaf.bound_name().map(str::to_string)),
    );
    names
}
