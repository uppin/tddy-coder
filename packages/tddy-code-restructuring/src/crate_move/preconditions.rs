use super::malformed;

use crate::{
    crate_move::{manifest_edits, moving},
    plan::RefactorKind,
};

use super::Result;

use crate::plan::RefactorOp;

use crate::registry::Workspace;

/// Every precondition [`resolve`] enforces before it consults rust-analyzer.
///
/// `restructure check` reported `no findings` on plans that `apply` then rejected outright — twice,
/// on the nested-module refusal and on the cluster one. Both decisions are made before the server
/// is spawned, so `check` can reach the same verdict statically and for free.
///
/// Returns one message per operation that cannot run, in plan order; empty when the plan's
/// cross-crate moves are all viable.
///
/// # Errors
///
/// Refuses when a file the preconditions must read cannot be.
pub fn unrunnable_moves(workspace: &Workspace<'_>, ops: &[RefactorOp]) -> Result<Vec<String>> {
    let mut findings = Vec::new();
    for op in ops {
        if op.op != RefactorKind::MoveModuleToCrate {
            continue;
        }
        if let Err(refusal) = move_preconditions(workspace, op) {
            findings.push(refusal.to_string());
        }
    }
    Ok(findings)
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
