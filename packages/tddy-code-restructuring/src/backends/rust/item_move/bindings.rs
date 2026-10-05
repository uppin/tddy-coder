//! Following a name a module only imports to the item it stands for.
//!
//! A path written `super::Name` in code that moves is respelled to reach the module `super` meant. When
//! that module has `Name` through a private `use` rather than defining it, the respelled path names a
//! private import from outside the module that holds it. The path has to name what the import
//! brings in.

use super::destination::{find_module, Lookup, Package};
use super::preflight::resolved_from;
use crate::crate_move::source_scan::items_of_module;
use crate::registry::Workspace;

/// Where `name` really lives when the module at `module` binds it only by a `use` that does not
/// rename it, as a path below the crate root ending in `name`. `None` when the module defines the
/// name itself, binds it some other way, or cannot be read: the path is then left naming the module.
pub(in crate::backends::rust) fn import_target(
    workspace: &Workspace<'_>,
    package: &Package,
    module: &[String],
    name: &str,
) -> Option<Vec<String>> {
    let Ok(Lookup::Found(found)) = find_module(workspace, package, module) else {
        return None;
    };
    let text = workspace.read(&found.file).ok()?;
    let items = items_of_module(&text[found.scope]);
    if items.defined.iter().any(|defined| defined == name)
        || items.children.iter().any(|child| child.name == name)
    {
        return None;
    }
    let leaf = items
        .uses
        .iter()
        .find(|leaf| leaf.alias.is_none() && leaf.bound_name() == Some(name))?;
    resolved_from(&leaf.segments, module)
}
