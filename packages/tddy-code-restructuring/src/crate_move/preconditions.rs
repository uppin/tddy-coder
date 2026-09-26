use super::malformed;

use crate::crate_move::{cluster, manifest_edits, moving};

use super::Result;

use crate::plan::RefactorOp;

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
            if let Err(refusal) = move_preconditions(workspace, &op.with_anchor(anchor.clone())) {
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
#[allow(dead_code)] // TODO(check-parity): `move_preconditions` reports this.
pub(crate) fn stays_behind_through_a_body(
    workspace: &Workspace<'_>,
    op: &RefactorOp,
) -> Result<Option<String>> {
    // TODO(check-parity): implement over `crate_move::survey`
    let _ = (workspace, op);
    todo!("check-parity: a body path to a module staying behind")
}

/// The destination's root already binds the moved module's name — a `mod` declaration, or a file
/// at the target path. Moving into it would be a **merge**, which no operation performs. Static: it
/// needs no index, so a plain `check` reports it.
#[allow(dead_code)] // TODO(check-parity): `move_preconditions` reports this.
pub(crate) fn destination_already_has_the_module(
    workspace: &Workspace<'_>,
    op: &RefactorOp,
) -> Result<Option<String>> {
    // TODO(check-parity): implement
    let _ = (workspace, op);
    todo!("check-parity: a module name the destination already has")
}

/// Every check [`resolve`] runs before it consults rust-analyzer.
pub(crate) fn move_preconditions(workspace: &Workspace<'_>, op: &RefactorOp) -> Result<()> {
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
    Ok(())
}
