use std::collections::BTreeSet;

use super::malformed;

use crate::crate_move::{cluster, header, manifest_edits, module_home, moving, survey};

use super::Result;

use crate::plan::{Anchor, RefactorOp};

use crate::registry::Workspace;

/// Every precondition [`resolve`] enforces before it consults rust-analyzer.
///
/// `restructure check` reported `no findings` on plans that `apply` then rejected outright — twice,
/// on the nested-module refusal and on the cluster one. Both decisions are made before the server
/// is spawned, so `check` can reach the same verdict statically and for free.
///
/// Returns one message per operation that cannot run, in plan order, and then what the plan as a
/// whole would strand; empty when the plan's cross-crate moves are all viable.
///
/// The second half is read across the plan rather than per operation, because that is the question:
/// whether a module's siblings are named by the *plan*, not by the operation moving it. An
/// operation is viable on its own and leaves a mutually-referencing set half moved.
///
/// # Errors
///
/// Refuses when a file the preconditions must read cannot be.
pub fn unrunnable_moves(workspace: &Workspace<'_>, ops: &[RefactorOp]) -> Result<Vec<String>> {
    let mut findings: Vec<String> = unrunnable(workspace, ops)?
        .into_iter()
        .map(|(_, refusal)| refusal)
        .collect();
    findings.extend(cluster::siblings_left_behind(workspace, ops)?);
    Ok(findings)
}

/// [`unrunnable_moves`]'s per-operation half, with each refusal tied to the operation it is about.
///
/// A reader fixes a plan by operation index and `check` reports one, so the index is carried here
/// and dropped by the published call — the same split [`cluster::stranded_siblings`] makes.
///
/// Every member of a cluster is checked, not only the module its anchor names: an operation moving
/// a set is unrunnable when any one of them cannot move.
///
/// # Errors
///
/// Refuses when a file the preconditions must read cannot be.
pub(crate) fn unrunnable(
    workspace: &Workspace<'_>,
    ops: &[RefactorOp],
) -> Result<Vec<(usize, String)>> {
    let mut findings = Vec::new();
    for (index, op) in ops.iter().enumerate() {
        if !op.op.moves_across_crates() {
            continue;
        }
        for anchor in op.anchors() {
            let earlier = moved_by_earlier_operations(workspace, ops, index, anchor);
            if let Err(refusal) = move_preconditions(workspace, &member_op(op, anchor), &earlier) {
                findings.push((index, refusal.to_string()));
            }
        }
    }
    Ok(findings)
}

/// A moved file reaching a module that stays behind **through a body path** — `crate::host::f(…)`
/// with no `use` line naming it — read from the move's path survey rather than from its header.
///
/// The header-only finding passed `check --deep` on exactly this shape, and the move then could not
/// build: after it, `crate::host` names nothing in the destination, and naming `origin` from there
/// is a cycle. The finding never suggests `move_cluster_to_crate` — a body's reach into the code that
/// hosts it is not a sibling that can come along.
///
/// A path into a module the operation itself moves travels with it, one into a module in
/// `earlier` — moved to this destination by an earlier operation of the plan — is already there by
/// the time the operation runs, and one the origin only re-exports from another crate is defined
/// elsewhere; none stays behind. A path under
/// `#[cfg(test)]` is no edge either, the same reading the header pass takes.
///
/// # Errors
///
/// Refuses when the survey does.
pub(crate) fn stays_behind_through_a_body(
    workspace: &Workspace<'_>,
    op: &RefactorOp,
    earlier: &BTreeSet<String>,
) -> Result<Option<String>> {
    let moving = moving::Move::read(workspace, op)?;
    let mut travelling = earlier.clone();
    for anchor in op.anchors() {
        travelling.insert(home_of_anchor(workspace, anchor)?.path.join("::"));
    }

    let text = workspace.read(&moving.source)?;
    let survey = survey::survey_moved_file(workspace, &text, &moving.origin, &moving.home.path)?;
    let origin = &moving.origin.extern_name;

    let left_behind = survey
        .paths
        .iter()
        .filter(|path| path.in_body && !path.in_test && path.defining_crate == *origin)
        .find_map(|path| {
            module_left_behind(&path.defined_at, origin, &travelling).map(|module| (path, module))
        });
    Ok(left_behind.map(|(path, module)| {
        format!(
            "`{source}` reaches `{written}` in a body at line {line}, and `{module}` stays behind \
             in `{origin}` — after the move that path names nothing in `{destination}`, and naming \
             `{origin}` from there is a cycle. Cut the body's dependency on `{module}` before moving \
             the module.",
            source = moving.source,
            written = path.written,
            line = path.site.line,
            origin = moving.origin.package,
            destination = moving.destination.package,
        )
    }))
}

/// The module of `origin` that `defined_at` is inside, when that module stays behind — `None` for a
/// path outside `origin`, for an item of the crate root (inside no module, and not this finding's to
/// name: `origin::host::project_root` is inside `host`, `origin::helper` is inside none), and for a
/// module that `travelling` carries along.
fn module_left_behind<'a>(
    defined_at: &'a str,
    origin: &str,
    travelling: &BTreeSet<String>,
) -> Option<&'a str> {
    let inside = defined_at.strip_prefix(&format!("{origin}::"))?;
    let mut segments = inside.split("::");
    let (module, _item) = (segments.next()?, segments.next()?);
    header::travels_with(inside, travelling)
        .is_none()
        .then_some(module)
}

/// The module `anchor` names, with where it is declared.
///
/// # Errors
///
/// Refuses when the anchor's file names no module or lies in no crate.
fn home_of_anchor(workspace: &Workspace<'_>, anchor: &Anchor) -> Result<module_home::ModuleHome> {
    let module = module_home::module_name(anchor.file())?;
    module_home::module_home(workspace, anchor.file(), &module)
}

/// The destination's root already binds the moved module's name — a `mod` declaration, or a file
/// at the target path. Moving into it would be a **merge**, which no operation performs. Static: it
/// needs no index, so a plain `check` reports it.
///
/// # Errors
///
/// Refuses when the destination's root cannot be read.
pub(crate) fn destination_already_has_the_module(
    workspace: &Workspace<'_>,
    op: &RefactorOp,
) -> Result<Option<String>> {
    let moving = moving::Move::read(workspace, op)?;
    let root = moving.destination_root();
    let declared = workspace.read(&root)?;
    if manifest_edits::module_declaration(&declared, &moving.module).is_some() {
        return Ok(Some(would_be_a_merge(
            &moving,
            &format!("declares `{}` in {root}", moving.module),
        )));
    }

    let target = moving.moved_to();
    if workspace.root.join(&target).exists() {
        return Ok(Some(would_be_a_merge(&moving, &format!("has {target}"))));
    }
    Ok(None)
}

/// The finding for a destination that `already` binds the moved module's name.
fn would_be_a_merge(moving: &moving::Move, already: &str) -> String {
    format!(
        "`{destination}` already {already} — moving `{module}` into it would be a merge, which no \
         operation performs",
        destination = moving.destination.package,
        module = moving.module,
    )
}

/// Every check [`resolve`] runs before it consults rust-analyzer.
///
/// `earlier` is what [`moved_by_earlier_operations`] reads for `op`'s place in its plan.
pub(crate) fn move_preconditions(
    workspace: &Workspace<'_>,
    op: &RefactorOp,
    earlier: &BTreeSet<String>,
) -> Result<()> {
    let moving = moving::Move::read(workspace, op)?;
    let text = workspace.read(&moving.home.declared_in)?;
    if manifest_edits::module_declaration(&text, &moving.module).is_none() {
        let where_declared = if moving.home.is_top_level() {
            "crate root"
        } else {
            "parent module"
        };
        return Err(malformed(format!(
            "{declared_in} declares no `mod {module}` — a module this {where_declared} does not \
             declare is not this crate's to move",
            declared_in = moving.home.declared_in,
            module = moving.module,
            where_declared = where_declared
        )));
    }
    if let Some(finding) = destination_already_has_the_module(workspace, op)? {
        return Err(malformed(finding));
    }
    if let Some(finding) = stays_behind_through_a_body(workspace, op, earlier)? {
        return Err(malformed(finding));
    }
    Ok(())
}

/// One member of a cluster as an operation of its own: addressed at `anchor`, with every other
/// anchor of the set in `also`, so [`RefactorOp::anchors`] still lists the whole set.
///
/// [`RefactorOp::with_anchor`] swaps the anchor and keeps `also`, which drops the original anchor
/// from the set when a member is checked — and a member's body reaching the module that anchors
/// the cluster is a path that travels with it.
pub(crate) fn member_op(op: &RefactorOp, anchor: &Anchor) -> RefactorOp {
    RefactorOp {
        also: op
            .anchors()
            .filter(|other| *other != anchor)
            .cloned()
            .collect(),
        ..op.with_anchor(anchor.clone())
    }
}

/// The module paths, in the crate `anchor`'s module leaves, that operations **before** `index`
/// already move to the same destination — what `crate::<module>` can still name there when
/// operation `index` runs.
///
/// The body check's counterpart of the header pass's `gone_by_then`: the same question, asked of the
/// plan's operations directly. That one reads the list of modules the plan moves, and that list is
/// built by running these preconditions, so it cannot be asked from inside them. An earlier
/// operation that is itself unrunnable still counts: it is reported on its own, and naming the
/// module it was meant to move again here would report one defect twice.
///
/// An anchor that is in no crate has no earlier operation to read.
pub(crate) fn moved_by_earlier_operations(
    workspace: &Workspace<'_>,
    ops: &[RefactorOp],
    index: usize,
    anchor: &Anchor,
) -> BTreeSet<String> {
    let home_of = |anchor: &Anchor| home_of_anchor(workspace, anchor).ok();
    let Some(here) = home_of(anchor) else {
        return BTreeSet::new();
    };
    ops[..index]
        .iter()
        .filter(|earlier| {
            earlier.op.moves_across_crates() && earlier.to.is_some() && earlier.to == ops[index].to
        })
        .flat_map(RefactorOp::anchors)
        .filter_map(home_of)
        .filter(|home| home.crate_dir == here.crate_dir)
        .map(|home| home.path.join("::"))
        .collect()
}
