//! Whether an item path lies in the module its file is — the half of resolving an item anchor that
//! needs no language server.
//!
//! One rule with two callers: the Rust resolver, which applies it before walking the outline, and
//! the static `check`, which applies it with no server at all. Sharing it is what makes the two
//! refuse a foreign prefix in the same words.

use std::path::Path;

use crate::plan::{Anchor, ItemPath, ItemSegment};
use crate::{RestructureError, Result};

/// The segments of `item` that lie below the module `file` is, refusing a path that is not in it.
pub(crate) fn segments_below(
    item: &ItemPath,
    module: &[String],
    file: &str,
) -> Result<Vec<ItemSegment>> {
    let segments = item.segments();
    let modules = &module[1..];
    let in_this_module = item.crate_name() == module[0]
        && segments.len() >= modules.len()
        && segments
            .iter()
            .zip(modules)
            .all(|(segment, name)| segment == &ItemSegment::Named(name.clone()));
    if !in_this_module {
        return Err(malformed(format!(
            "`{item}` is not in {file}, which is module `{}`",
            module.join("::")
        )));
    }

    let below = segments[modules.len()..].to_vec();
    if below.is_empty() {
        return Err(malformed(format!(
            "`{item}` names the module {file} is, not an item in it"
        )));
    }
    Ok(below)
}

/// Refuse `anchor` when an item it names does not lie in the module its file is.
///
/// An `item` anchor's path, or each of an `items` anchor's paths in order, is read against
/// [`super::module_path_of`] of the anchor's file; the first that does not lie in it is refused in
/// [`segments_below`]'s words, and `module_path_of`'s own refusals (a file outside `src/`, or in
/// no package) pass through. A `symbol` or `range` anchor names no item and passes.
pub(crate) fn refuse_a_foreign_prefix(root: &Path, anchor: &Anchor) -> Result<()> {
    // TODO(reshape-anchors-outline): implement — read the file's module path and apply
    // `segments_below` to every item the anchor names (changeset R2). Until then nothing is
    // refused, which is what a static check did before this node.
    let _ = (root, anchor);
    Ok(())
}

fn malformed(reason: String) -> RestructureError {
    RestructureError::MalformedPlan(reason)
}
