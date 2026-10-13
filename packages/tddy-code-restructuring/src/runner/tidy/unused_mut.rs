//! The tidy's `unused_mut` step: rustc's own "remove this `mut`", in the files a run wrote.
//!
//! rust-analyzer reports no unused `mut` (not even with its experimental diagnostics), so the one
//! an `extract_method` copies into the new function — or leaves in its caller — is the compiler's
//! to find. It runs after the unused imports and before `rustfmt`, applies `MachineApplicable`
//! suggestions only, and undoes itself from the bytes held in memory when the re-check rejects
//! them, failing the run as the import step does.

use std::collections::{BTreeMap, BTreeSet};

use super::diagnostics::{Diagnostic, Fix};
use super::{apply_fixes, check, restore, Checked, Failure, Tidying};
use crate::Result;

/// What the step did.
pub(super) enum Removed {
    /// No touched file held an unused `mut`.
    Nothing,
    /// Removed, and the re-checked tree compiles; its diagnostics.
    Compiles(Vec<Diagnostic>),
    /// Removed, the re-check failed, and the files were restored.
    Broken(Failure),
}

/// Remove every `unused_mut` that `diagnostics` (the last check's) reports with a
/// `MachineApplicable` fix in a file `tidying` touched, then re-check.
pub(super) fn remove_unused_mut(
    tidying: &Tidying<'_>,
    diagnostics: &[Diagnostic],
) -> Result<Removed> {
    let fixes = unused_mut_fixes(diagnostics, tidying.touched);
    if fixes.is_empty() {
        return Ok(Removed::Nothing);
    }
    let mut before = BTreeMap::new();
    for file in fixes.keys() {
        before.insert(file.clone(), std::fs::read(tidying.root.join(file))?);
    }
    apply_fixes(tidying.root, &fixes)?;

    match check(tidying)? {
        Checked::Compiles(rechecked) => {
            for (file, removed) in &fixes {
                (tidying.progress)(&format!(
                    "tidied: removed {} unused `mut`(s) from {file}",
                    removed.len()
                ));
            }
            Ok(Removed::Compiles(rechecked))
        }
        Checked::Broken(failure) => {
            restore(tidying.root, &before)?;
            Ok(Removed::Broken(failure))
        }
    }
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
