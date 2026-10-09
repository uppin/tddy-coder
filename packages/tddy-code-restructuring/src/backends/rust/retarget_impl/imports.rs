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

#[cfg(test)]
mod tests {
    use super::super::super::item_move::use_path::on_disk::{an_app_holding, AnAppOnDisk};
    use super::*;

    /// An `app` with `svc::{host, other, roster}`, where `svc.rs` reads `svc_text` and `svc/host.rs`
    /// reads `host_text`.
    fn an_app_whose_svc_reads(svc_text: &str, host_text: &str) -> AnAppOnDisk {
        an_app_holding(&[
            ("src/lib.rs", "pub mod svc;\n"),
            ("src/svc.rs", svc_text),
            ("src/svc/host.rs", host_text),
            ("src/svc/other.rs", "pub struct Roster;\n"),
            ("src/svc/roster.rs", "pub struct Roster;\n"),
        ])
    }

    const SVC: &str = "pub mod host;\npub mod other;\npub mod roster;\n";

    /// The `use` a retarget to `app::svc::roster::Roster` writes into `file`, or why it refuses.
    fn the_use_written_into(
        app: &AnAppOnDisk,
        file: &str,
    ) -> std::result::Result<Option<String>, String> {
        app.reading(|workspace| {
            the_use(workspace, file, "app::svc::roster::Roster")
                .map_err(|refusal| refusal.to_string())
        })
    }

    #[test]
    fn a_super_import_of_the_new_type_needs_no_second_use() {
        // Given a `host` that imports `Roster` by a path through its parent
        let app = an_app_whose_svc_reads(SVC, "use super::roster::Roster;\n");

        // When a retarget to `app::svc::roster::Roster` asks which `use` it needs there
        let written = the_use_written_into(&app, "src/svc/host.rs");

        // Then none: the file already binds that very type
        assert_eq!(written, Ok(None));
    }

    #[test]
    fn a_self_or_child_relative_import_of_the_new_type_needs_no_second_use() {
        // Given a `svc` that imports `Roster` from its child as `self::roster::Roster`, and one that
        // writes the 2018 child-relative `roster::Roster`
        let through_self = an_app_whose_svc_reads(&format!("{SVC}use self::roster::Roster;\n"), "");
        let child_relative = an_app_whose_svc_reads(&format!("{SVC}use roster::Roster;\n"), "");

        // When a retarget to `app::svc::roster::Roster` asks which `use` each needs
        let (self_written, child_written) = (
            the_use_written_into(&through_self, "src/svc.rs"),
            the_use_written_into(&child_relative, "src/svc.rs"),
        );

        // Then neither needs one
        assert_eq!(self_written, Ok(None));
        assert_eq!(child_written, Ok(None));
    }

    #[test]
    fn an_import_of_another_item_or_another_crate_under_the_name_is_still_refused() {
        // Given a `host` binding `Roster` to a sibling's other type, and one binding it to a crate's
        let other_item = an_app_whose_svc_reads(SVC, "use super::other::Roster;\n");
        let other_crate = an_app_whose_svc_reads(SVC, "use kernel::Roster;\n");

        // When a retarget to `app::svc::roster::Roster` asks which `use` each needs
        let (item_refusal, crate_refusal) = (
            the_use_written_into(&other_item, "src/svc/host.rs"),
            the_use_written_into(&other_crate, "src/svc/host.rs"),
        );

        // Then both are refused as a clash, naming the name and the file
        for refusal in [item_refusal, crate_refusal] {
            let refusal = refusal.expect_err("the second binding is refused");
            assert!(
                refusal.contains("`Roster` is already bound in `src/svc/host.rs`")
                    && refusal.contains("E0255"),
                "unexpected refusal: {refusal}"
            );
        }
    }
}
