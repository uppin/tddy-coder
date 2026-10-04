//! Who can see a module: the visibility of the declarations a move's callers and imports pass through.
//!
//! A module is declared with the visibility its first user needed. A move that brings it a caller
//! further out, or brings code that names a module its own parent keeps private, has to widen
//! those declarations — by as little as the new reach needs — or the tree stops compiling. The
//! visibilities are read as module subtrees ([`Scope`]), so "as little as needed" is the common
//! ancestor of what the declaration covers and who now has to see it.

use std::collections::BTreeMap;
use std::ops::Range;

use super::super::module_reparent::declaration;
use super::assemble::{users_of, Moving};
use super::creation;
use super::destination::{find_module, Lookup, Package};
use super::scope::Scope;
use super::text::{identifiers_in, Edit};
use crate::crate_move::source_scan::{items_of_module, UseLeaf};
use crate::registry::Workspace;
use crate::Result;

/// An edit to the file at the path.
pub(super) type FileEdit = (String, Edit);

/// Who has to be able to see a module.
pub(super) enum Needs {
    /// Something outside the crate does.
    Public,
    /// These modules, below the crate root, do.
    Modules(Vec<Vec<String>>),
}

impl Needs {
    /// `scope`, widened until it covers everyone this needs.
    fn apply(&self, scope: Scope) -> Scope {
        match self {
            Needs::Public => Scope::Public,
            Needs::Modules(modules) => modules
                .iter()
                .fold(scope, |scope, module| scope.widened_to(module)),
        }
    }
}

/// Who has to see the destination because of the callers the move re-points and the facade it
/// leaves in the source module.
pub(super) fn callers_need(
    moving: &Moving<'_>,
    texts: &BTreeMap<String, String>,
    region: &Range<usize>,
    facade: bool,
) -> Needs {
    let mut modules = Vec::new();
    if facade {
        modules.push(moving.source.to_vec());
    }
    if moving.reexport.repoints_callers() {
        for item in &moving.run.items {
            match users_of(moving, texts, region, &item.name) {
                Some(users) => modules.extend(users),
                None => return Needs::Public,
            }
        }
    }
    Needs::Modules(modules)
}

/// What the move has to do so its callers can reach the destination: declare the module it creates,
/// and widen the declarations of the modules on the way to it.
pub(super) fn reach_the_destination(
    moving: &Moving<'_>,
    texts: &mut BTreeMap<String, String>,
    region: &Range<usize>,
    facade: bool,
    edits: &mut BTreeMap<String, Vec<Edit>>,
) -> Result<()> {
    let needs = callers_need(moving, texts, region, facade);
    let on_the_way = match moving.created {
        Some((parent, name)) => {
            let visibility = needs
                .apply(Scope::Within(parent.path.clone()))
                .spelled_in(&parent.path);
            let (at, written) =
                creation::declaration(&texts[&parent.file], &parent.scope, &visibility, name);
            edits
                .entry(parent.file.clone())
                .or_default()
                .push(Edit::insert(at, written));
            parent.path.as_slice()
        }
        None => moving.destination.path.as_slice(),
    };
    let widenings =
        widen_declarations(moving.workspace, moving.package, texts, on_the_way, &needs)?;
    for (path, edit) in widenings {
        edits.entry(path).or_default().push(edit);
    }
    Ok(())
}

/// The edits that widen, as far as `needs` requires, the declaration of every module on the way to
/// the module at `path`. Empty when each already reaches everyone.
pub(super) fn widen_declarations(
    workspace: &Workspace<'_>,
    package: &Package,
    texts: &mut BTreeMap<String, String>,
    path: &[String],
    needs: &Needs,
) -> Result<Vec<FileEdit>> {
    let mut edits = Vec::new();
    for depth in 1..=path.len() {
        let Lookup::Found(parent) = find_module(workspace, package, &path[..depth - 1])? else {
            continue;
        };
        if !texts.contains_key(&parent.file) {
            texts.insert(parent.file.clone(), workspace.read(&parent.file)?);
        }
        let Some(found) = declaration::find(&texts[&parent.file], &parent.scope, &path[depth - 1])?
        else {
            continue;
        };
        let Some(scope) = Scope::parse(&found.visibility, &parent.path) else {
            continue;
        };
        let wider = needs.apply(scope.clone());
        if wider != scope {
            let spelled = wider.spelled_in(&parent.path);
            let written = if spelled.is_empty() {
                spelled
            } else {
                format!("{spelled} ")
            };
            edits.push((
                parent.file.clone(),
                Edit::replace(found.visibility_span.clone(), written),
            ));
        }
    }
    Ok(edits)
}

/// `lines` (the `use` items copied into the destination) that the destination can reach, and the
/// widenings the ones it needs but cannot yet reach call for.
///
/// An import of a module the destination cannot see is dropped when the moved code names nothing
/// from it, because the unused-import tidy would remove it anyway, and otherwise the module's
/// declaration is widened: the moved code genuinely reaches it.
pub(super) fn reachable_imports(
    moving: &Moving<'_>,
    texts: &mut BTreeMap<String, String>,
    moved: &str,
    lines: Vec<String>,
) -> Result<(Vec<String>, Vec<FileEdit>)> {
    let mentioned = identifiers_in(moved);
    let needs = Needs::Modules(vec![moving.destination.path.clone()]);
    let (mut kept, mut edits) = (Vec::new(), Vec::new());
    for line in lines {
        let (mut blocked, mut needed) = (false, false);
        let mut widenings = Vec::new();
        for leaf in &items_of_module(&line).uses {
            let Some(module) = module_of(moving, leaf)? else {
                continue;
            };
            let found =
                widen_declarations(moving.workspace, moving.package, texts, &module, &needs)?;
            if found.is_empty() {
                continue;
            }
            blocked = true;
            needed |= names_something_moved_code_mentions(moving, leaf, &module, &mentioned)?;
            widenings.extend(found);
        }
        if blocked && !needed {
            continue;
        }
        edits.extend(widenings);
        kept.push(line);
    }
    Ok((kept, edits))
}

/// The module of this crate a `use` leaf goes through, below the crate root. `None` for a path that
/// is not rooted at `crate`, which another crate's visibility answers.
fn module_of(moving: &Moving<'_>, leaf: &UseLeaf) -> Result<Option<Vec<String>>> {
    let [first, rest @ ..] = leaf.segments.as_slice() else {
        return Ok(None);
    };
    if first != "crate" || rest.is_empty() {
        return Ok(None);
    }
    let whole = matches!(
        find_module(moving.workspace, moving.package, rest)?,
        Lookup::Found(_)
    );
    let module = if leaf.glob || whole {
        rest
    } else {
        &rest[..rest.len() - 1]
    };
    Ok((!module.is_empty()).then(|| module.to_vec()))
}

/// Whether the moved code names what the leaf brings in: the name it binds, or, for a glob, any
/// name the module it globs defines.
fn names_something_moved_code_mentions(
    moving: &Moving<'_>,
    leaf: &UseLeaf,
    module: &[String],
    mentioned: &std::collections::BTreeSet<String>,
) -> Result<bool> {
    if let Some(name) = leaf.bound_name() {
        return Ok(mentioned.contains(name));
    }
    let Lookup::Found(found) = find_module(moving.workspace, moving.package, module)? else {
        return Ok(true);
    };
    let text = moving.workspace.read(&found.file)?;
    Ok(items_of_module(&text[found.scope])
        .defined
        .iter()
        .any(|name| mentioned.contains(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(path: &str) -> Vec<String> {
        path.split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn widens_a_private_declaration_to_the_ancestor_that_covers_a_new_caller() {
        let needs = Needs::Modules(vec![module("handler")]);

        assert_eq!(
            needs.apply(Scope::Within(module("host"))),
            Scope::Within(module(""))
        );
    }

    #[test]
    fn leaves_a_declaration_that_already_covers_every_caller_alone() {
        let needs = Needs::Modules(vec![module("host::inner")]);

        assert_eq!(
            needs.apply(Scope::Within(module("host"))),
            Scope::Within(module("host"))
        );
    }

    #[test]
    fn makes_a_declaration_public_for_a_caller_in_another_crate() {
        assert_eq!(
            Needs::Public.apply(Scope::Within(module("host"))),
            Scope::Public
        );
    }
}
