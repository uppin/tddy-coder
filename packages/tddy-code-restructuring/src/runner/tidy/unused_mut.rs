//! The tidy's `unused_mut` step: rustc's own "remove this `mut`", in the files a run wrote.
//!
//! rust-analyzer reports no unused `mut` (not even with its experimental diagnostics), so the one
//! an `extract_method` copies into the new function — or leaves in its caller — is the compiler's
//! to find. It runs after the unused imports and before `rustfmt`, applies `MachineApplicable`
//! suggestions only, and undoes itself from the bytes held in memory when the re-check rejects
//! them, failing the run as the import step does.

use super::diagnostics::Diagnostic;
use super::{Failure, Tidying};
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
    let _ = (tidying, diagnostics);
    // TODO(reshape-extract-method-clean): implement, and call it from `tidy` before `rustfmt`
    todo!("remove_unused_mut")
}
