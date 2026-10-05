//! The bulk form's references: every call of one method the server knows, classified.

use super::RustBackend;
use crate::edit::Resolution;
use crate::plan::RefactorOp;
use crate::registry::Workspace;
use crate::Result;

/// Insert the template's hops after the receiver of every call of the anchored method, in every
/// file the server knows, refusing every reference that is not a method call all at once and
/// writing nothing.
///
/// TODO(repoint-call): implement over `RustBackend::sites_of` (classify each site from the masked
/// text around the name: method call, comment, anything else), `receivers::insertions_for`, and the
/// apply note counting the calls re-pointed.
pub(super) fn repoint_receivers(
    _backend: &mut RustBackend,
    _op: &RefactorOp,
    _workspace: &Workspace<'_>,
) -> Result<Resolution> {
    Err(super::unfinished())
}
