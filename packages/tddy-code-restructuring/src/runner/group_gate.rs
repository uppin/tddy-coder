//! The gate at a transactional group's end, and the rollback when the group does not pass it.
//!
//! Some refactors cannot compile step by step — change a type, then adapt every use — so a plan
//! marks consecutive operations as one group. Ungrouped operations keep the end-of-run gate
//! ([`super::refuse_a_broken_result`]) and its leave-on-disk contract; a group is judged at its own
//! end instead, by `cargo check --all-targets` over the packages its members touched, and a group
//! that fails there is rolled back exactly from the pre-images its members journalled
//! ([`crate::journal::PreImage`]): contents rewritten, created files removed, renames undone.
//! Everything before the group stays applied.
//!
//! Both apply loops call these — the command line's ([`super::apply_from_store`]) and the index
//! daemon's — and a resume that finds a group open ([`Journal::open_group`]) rolls it back before
//! it runs anything.

use std::path::Path;

use tokio_util::sync::CancellationToken;

use crate::journal::Journal;
use crate::Result;

use super::StatePaths;

/// Refuse the group `group` when the tree its members left does not compile.
///
/// Checked over the packages owning every file the group's members journalled an edit to.
///
/// # Errors
///
/// [`crate::RestructureError::GroupDoesNotCompile`] naming the group and the compiler's errors. The
/// caller rolls the group back ([`roll_back_group`]) before returning it.
pub fn gate_group(
    _root: &Path,
    _group: &str,
    _journal: &Journal,
    _cancel: &CancellationToken,
) -> Result<()> {
    todo!(
        "TODO(transactional-groups): cargo check --all-targets over the packages the group touched"
    )
}

/// Restore every file the group `group` touched from the pre-images its members journalled, then
/// journal `group_rolled_back`.
///
/// Pre-images are restored in reverse of the order they were written, so a file a later member
/// touched first is restored from what the group first saw.
pub fn roll_back_group(
    _root: &Path,
    _group: &str,
    _paths: &StatePaths,
    _journal: &mut Journal,
) -> Result<()> {
    todo!(
        "TODO(transactional-groups): restore the group's pre-images and journal group_rolled_back"
    )
}
