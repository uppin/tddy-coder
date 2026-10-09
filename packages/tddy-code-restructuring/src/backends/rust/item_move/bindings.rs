//! Following a name a module only imports to the item it stands for.
//!
//! A path written `super::Name` in code that moves is respelled to reach the module `super` meant. When
//! that module has `Name` through a private `use` rather than defining it, the respelled path names a
//! private import from outside the module that holds it. The path has to name what the import
//! brings in — one hop, as the import writes it (rules R3–R8 of the `#reshape` 11/19 changeset).

use super::destination::{find_module, Lookup, Package};
use super::preflight::resolved_from;
use crate::crate_move::source_scan::items_of_module;
use crate::registry::Workspace;

/// How a module binds a name it does not define, as seen from the module code arrives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::backends::rust) enum Imported {
    /// The module's import is visible at the destination (`pub`, `pub(crate)`, or a `pub(in …)` whose
    /// scope holds it): the path to the module, as written, still works (R3).
    #[allow(
        dead_code,
        reason = "TODO(reshape-move-item-paths): answered by `import_target` at green"
    )]
    Visible,
    /// The import brings in `name` from the module `module` of this crate (below the crate root): a
    /// plain `use` (R4), an aliased one whose real name is `name` (R5), or the one glob whose module
    /// binds the name (R6).
    InCrate { module: Vec<String>, name: String },
    /// The import names another crate: `path` is the crate-rooted path of what it brings in, the
    /// crate's name first; `shadowed` when the destination module binds the crate's name itself (R7).
    #[allow(
        dead_code,
        reason = "TODO(reshape-move-item-paths): answered by `import_target` at green"
    )]
    Extern { path: Vec<String>, shadowed: bool },
    /// The module binds the name only through globs that cannot confirm it, and why (R8).
    #[allow(
        dead_code,
        reason = "TODO(reshape-move-item-paths): answered by `import_target` at green"
    )]
    Unconfirmed(String),
}

/// How the module at `module` binds `name` when it does not define it, for code arriving in
/// `destination` (both below the crate root). `None` when the module defines the name itself,
/// declares it as a child module, or cannot be read: the path is then left naming the module.
pub(in crate::backends::rust) fn import_target(
    workspace: &Workspace<'_>,
    package: &Package,
    module: &[String],
    name: &str,
    destination: &[String],
) -> Option<Imported> {
    // TODO(reshape-move-item-paths): implement R3 (visible import), R5 (alias), R6 (confirmed glob),
    // R7 (extern head) and R8 (unconfirmed glob) over `use_path::resolved`; today only R4.
    let _ = destination;
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
    let mut target = resolved_from(&leaf.segments, module)?;
    let real = target.pop()?;
    Some(Imported::InCrate {
        module: target,
        name: real,
    })
}

#[cfg(test)]
mod tests {
    use super::super::destination::package_of;
    use super::super::use_path::on_disk::{an_app_holding, AnAppOnDisk};
    use super::*;

    fn path(text: &str) -> Vec<String> {
        text.split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// An `app` whose crate root declares `host`, `split` and `types`, `host` reading `host_text`.
    fn an_app_whose_host_reads(host_text: &str) -> AnAppOnDisk {
        an_app_holding(&[
            (
                "src/lib.rs",
                "pub mod a;\npub mod b;\npub mod host;\npub mod split;\npub mod types;\n",
            ),
            ("src/host.rs", host_text),
            ("src/types.rs", "pub struct Config;\npub struct Real;\n"),
            ("src/a.rs", "pub struct Config;\n"),
            ("src/b.rs", "pub struct Config;\n"),
            ("src/split.rs", "pub fn marker() {}\n"),
        ])
    }

    /// How `host` binds `name`, for code arriving in `destination`.
    fn how_host_binds(app: &AnAppOnDisk, name: &str, destination: &str) -> Option<Imported> {
        app.reading(|workspace| {
            let package = package_of(workspace.root, "src/host.rs").expect("the package reads");
            import_target(workspace, &package, &path("host"), name, &path(destination))
        })
    }

    fn in_crate(module: &str, name: &str) -> Option<Imported> {
        Some(Imported::InCrate {
            module: path(module),
            name: name.to_string(),
        })
    }

    #[test]
    fn a_pub_crate_import_reachable_from_the_destination_is_visible() {
        // Given a `host` that re-exports `Config` to the whole crate
        let app = an_app_whose_host_reads("pub(crate) use crate::types::Config;\n");

        // When code arriving in `split` asks how `host` binds it
        let bound = how_host_binds(&app, "Config", "split");

        // Then the path to `host`, as written, still works there
        assert_eq!(bound, Some(Imported::Visible));
    }

    #[test]
    fn a_restricted_import_the_destination_is_outside_of_is_followed() {
        // Given a `host` that re-exports `Config` to its own subtree only, and a destination in `split`
        let app = an_app_whose_host_reads("pub(in crate::host) use crate::types::Config;\n");

        // When code arriving in `split::deep` asks how `host` binds it
        let bound = how_host_binds(&app, "Config", "split::deep");

        // Then the import is followed to the module that holds `Config`
        assert_eq!(bound, in_crate("types", "Config"));
    }

    #[test]
    fn an_aliased_import_answers_the_name_it_brings_in() {
        // Given a `host` that imports `Real` under the name `Settings`
        let app = an_app_whose_host_reads("use crate::types::Real as Settings;\n");

        // When code arriving in `split` asks how `host` binds `Settings`
        let bound = how_host_binds(&app, "Settings", "split");

        // Then it is `Real`, in `types`
        assert_eq!(bound, in_crate("types", "Real"));
    }

    #[test]
    fn an_extern_import_answers_the_crates_path_and_whether_the_destination_shadows_it() {
        // Given a `host` that privately imports the module `util` of the crate `kernel`, and a
        // destination `split` that declares a module named `kernel` of its own
        let app = an_app_whose_host_reads("use kernel::util;\n");
        let shadowing = an_app_holding(&[
            ("src/lib.rs", "pub mod host;\npub mod split;\n"),
            ("src/host.rs", "use kernel::util;\n"),
            ("src/split.rs", "pub mod kernel {}\n"),
        ]);

        // When code arriving in `split` asks how `host` binds `util`, in each
        let plain = how_host_binds(&app, "util", "split");
        let shadowed = how_host_binds(&shadowing, "util", "split");

        // Then it is the crate's own path, rooted when the destination shadows the crate's name
        assert_eq!(
            plain,
            Some(Imported::Extern {
                path: path("kernel::util"),
                shadowed: false,
            })
        );
        assert_eq!(
            shadowed,
            Some(Imported::Extern {
                path: path("kernel::util"),
                shadowed: true,
            })
        );
    }

    #[test]
    fn a_glob_counts_only_when_exactly_one_glob_module_binds_the_name() {
        // Given a `host` globbing `types`, one globbing `types` and `split`, and one globbing `a`
        // and `b`, which both define `Config`
        let one = an_app_whose_host_reads("use crate::types::*;\nuse crate::split::*;\n");
        let none = an_app_whose_host_reads("use crate::split::*;\n");
        let two = an_app_whose_host_reads("use crate::a::*;\nuse crate::b::*;\n");

        // When code arriving in `split` asks how each binds `Config`
        let confirmed = how_host_binds(&one, "Config", "split");
        let unconfirmed = how_host_binds(&none, "Config", "split");
        let ambiguous = how_host_binds(&two, "Config", "split");

        // Then only the one confirming glob is followed; none and two cannot be
        assert_eq!(confirmed, in_crate("types", "Config"));
        assert!(
            matches!(unconfirmed, Some(Imported::Unconfirmed(_))),
            "a glob that does not bind the name was followed: {unconfirmed:?}"
        );
        assert!(
            matches!(ambiguous, Some(Imported::Unconfirmed(_))),
            "two globs that bind the name were not refused: {ambiguous:?}"
        );
    }
}
