//! The tidy's `unused_mut` step: rustc's own "remove this `mut`", in the files a run wrote.
//!
//! rust-analyzer reports no unused `mut` (not even with its experimental diagnostics), so the one
//! an `extract_method` copies into the new function — or leaves in its caller — is the compiler's
//! to find. It runs after the unused imports and before `rustfmt`, applies `MachineApplicable`
//! suggestions only, and undoes itself from the bytes held in memory when the re-check rejects
//! them, failing the run as the import step does.

use std::collections::{BTreeMap, BTreeSet};

use super::diagnostics::{Diagnostic, Fix};
use super::{apply_fixes, check, restore, Checked, Tidying};
use crate::Result;

/// `checked` after the step: removes every `unused_mut` that its diagnostics report with a
/// `MachineApplicable` fix in a file `tidying` touched, then re-checks.
///
/// A check that did not compile, or one with no such `mut`, is returned as it was. A re-check that
/// rejects the removals restores the files and is the failure the run reports.
pub(super) fn without_unused_mut(tidying: &Tidying<'_>, checked: Checked) -> Result<Checked> {
    let Checked::Compiles(diagnostics) = &checked else {
        return Ok(checked);
    };
    let fixes = unused_mut_fixes(diagnostics, tidying.touched);
    if fixes.is_empty() {
        return Ok(checked);
    }
    let mut before = BTreeMap::new();
    for file in fixes.keys() {
        before.insert(file.clone(), std::fs::read(tidying.root.join(file))?);
    }
    apply_fixes(tidying.root, &fixes)?;

    let rechecked = check(tidying)?;
    match &rechecked {
        Checked::Compiles(_) => {
            for (file, removed) in &fixes {
                (tidying.progress)(&format!(
                    "tidied: removed {} unused `mut`(s) from {file}",
                    removed.len()
                ));
            }
        }
        Checked::Broken(_) => restore(tidying.root, &before)?,
    }
    Ok(rechecked)
}

/// The removals the compiler offers for an unused `mut` in the touched files. Each target (library,
/// tests) repeats a diagnostic for a file they share, so edits are kept as sets.
fn unused_mut_fixes(
    diagnostics: &[Diagnostic],
    touched: &BTreeSet<String>,
) -> BTreeMap<String, BTreeSet<Fix>> {
    let mut fixes: BTreeMap<String, BTreeSet<Fix>> = BTreeMap::new();
    for diagnostic in diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("unused_mut"))
    {
        for fix in diagnostic
            .fixes
            .iter()
            .filter(|fix| touched.contains(&fix.file))
        {
            fixes
                .entry(fix.file.clone())
                .or_default()
                .insert(fix.clone());
        }
    }
    fixes
}
