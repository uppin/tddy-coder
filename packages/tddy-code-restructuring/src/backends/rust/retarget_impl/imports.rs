//! The one `use` a retarget writes, and the clash it refuses.
//!
//! The new type's module is brought into the file as `use crate::<module>::<Name>;`, placed by the
//! same rule a move uses. Nothing is written when the file already binds the name to that path, or
//! when the file is the module that declares the type; a file that binds the name to something else
//! is refused (S6), because a second binding is `E0255`.

use super::super::item_move::destination::package_of;
use super::super::item_move::preflight::names_declared_in;
use super::super::seam_refusal;
use super::preflight::ToType;
use crate::crate_move::source_scan::items_of_module;
use crate::item_anchor::module_path_of;
use crate::registry::Workspace;
use crate::Result;

/// The `use` line that brings the new type into `file`, `None` when it needs none, or the refusal
/// when the name is already bound to something else.
pub(super) fn the_use(
    workspace: &Workspace<'_>,
    file: &str,
    to_type: &str,
) -> Result<Option<String>> {
    let Some(named) = ToType::parse(to_type) else {
        return Ok(None);
    };
    let package = package_of(workspace.root, file)?;

    // The file that declares the new type needs no import of it.
    let mut declaring = vec![package.crate_name.clone()];
    declaring.extend(named.module.iter().cloned());
    if module_path_of(workspace.root, file)? == declaring {
        return Ok(None);
    }

    let path: Vec<String> = std::iter::once("crate".to_string())
        .chain(named.module.iter().cloned())
        .chain(std::iter::once(named.name.clone()))
        .collect();
    let text = workspace.read(file)?;
    let bound = items_of_module(&text)
        .uses
        .iter()
        .filter(|leaf| leaf.bound_name() == Some(named.name.as_str()))
        .any(|leaf| !leaf.glob && leaf.segments == path);

    if bound {
        return Ok(None);
    }
    if names_declared_in(&text).contains(&named.name) {
        return Err(seam_refusal(format!(
            "`{}` is already bound in `{file}` to something else: a `use` of `{}` would clash \
             (`E0255`)",
            named.name,
            path.join("::")
        )));
    }

    Ok(Some(format!("use {};", path.join("::"))))
}
