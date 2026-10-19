//! Rules 1 and 4: the declarations a reference from outside the moving files reaches, and the ones
//! never touched.

use std::collections::BTreeSet;

use super::{written_visibility, Widened};
use crate::crate_move::{DeclarationKind, ItemReferences, MovingCluster, Result, Survey};
use crate::registry::Workspace;

/// Every `Item`, `Field` and `InherentMember` declaration of a moving file, not already `pub`, that
/// a file the move does not carry (`moving` holds the carried ones) and that is not in the
/// destination crate references — and each inline `mod` on a reached item's path. `NoVisibility`
/// declarations are never returned.
///
/// # Errors
///
/// Refuses when a moving file cannot be read, or a reached declaration's keyword is on a line above
/// its name.
pub(crate) fn reached(
    workspace: &Workspace<'_>,
    cluster: &MovingCluster,
    moving: &BTreeSet<String>,
    surveys: &[Survey],
) -> Result<Vec<Widened>> {
    let mut found = Vec::new();
    for survey in surveys {
        let text = workspace.read(&survey.source)?;
        let reaching = survey
            .declarations
            .iter()
            .filter(|declaration| declaration.kind != DeclarationKind::NoVisibility)
            .filter(|declaration| reaches_from_outside(declaration, moving, cluster));
        for declaration in reaching {
            for on_the_way in with_its_inline_modules(declaration, &survey.declarations) {
                let written = written_visibility(&survey.source, &text, on_the_way)?;
                if written.is_some_and(|written| written != "pub") {
                    found.push(Widened::of(&survey.source, on_the_way, None));
                }
            }
        }
    }
    Ok(found)
}

/// Whether a file outside the moving files and outside the destination crate names `declaration`:
/// a reference from inside the destination was already crossing a crate boundary.
fn reaches_from_outside(
    declaration: &ItemReferences,
    moving: &BTreeSet<String>,
    cluster: &MovingCluster,
) -> bool {
    let destination = format!("{}/", cluster.destination.dir.trim_end_matches('/'));
    declaration.referenced_at.iter().any(|reference| {
        !moving.contains(&reference.path) && !reference.path.starts_with(&destination)
    })
}

/// The inline `mod` declarations on the path of an item of an inline module, outermost first, and
/// then the declaration itself. A field or a member sits in a type, not a module, so nothing is on
/// its way.
fn with_its_inline_modules<'a>(
    declaration: &'a ItemReferences,
    all: &'a [ItemReferences],
) -> Vec<&'a ItemReferences> {
    let modules = (0..declaration.within.len())
        .filter(|_| declaration.kind == DeclarationKind::Item)
        .filter_map(|depth| {
            all.iter().find(|candidate| {
                candidate.kind == DeclarationKind::Item
                    && candidate.item == declaration.within[depth]
                    && candidate.within == declaration.within[..depth]
            })
        });
    modules.chain([declaration]).collect()
}
