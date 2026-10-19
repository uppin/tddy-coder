//! The one reading of a `use` path: which module or item of this crate it names, or which crate.
//!
//! Rust 2018 resolves a `use` path's head in the module that writes it. `crate`, `self` and `super`
//! are relative to that module; any other head is a name the module binds (a `mod` it declares, an
//! item it defines, a name it imports) or, when it binds none, an **extern crate**. Reading every
//! other head as relative is what turned `pub use tddy_session_split::service_util;` into a child
//! module of the importing module and wrote `super::tddy_session_split::…` (`#carve` 21/21).
//!
//! The reading is lexical: the module's own top-level text, as [`items_of_module`] reads it, is all
//! it consults.
//!
//! [`items_of_module`]: crate::crate_move::source_scan::items_of_module

use crate::crate_move::source_scan::ModuleItems;

/// What a `use` path names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::backends::rust) enum UsePath {
    /// A path below this crate's root, outermost first.
    InCrate(Vec<String>),
    /// A path rooted in another crate, the crate's name first.
    Extern(Vec<String>),
}

/// The path `segments`, written in a `use` of the module at `at` (below the crate root), names.
///
/// `local` is that module's own top level. `None` when a `super` climbs above the crate root.
pub(in crate::backends::rust) fn resolved(
    segments: &[String],
    at: &[String],
    local: &ModuleItems,
) -> Option<UsePath> {
    let head = segments.first()?;
    let start = match head.as_str() {
        "crate" => Vec::new(),
        "self" | "super" => at.to_vec(),
        name if binds(local, name) => at.to_vec(),
        _ => return Some(UsePath::Extern(segments.to_vec())),
    };
    walked(start, segments).map(UsePath::InCrate)
}

/// Whether the module whose top level is `local` binds `name`: it declares a child module, defines an
/// item or imports it by a non-glob `use`.
fn binds(local: &ModuleItems, name: &str) -> bool {
    local.defined.iter().any(|defined| defined == name)
        || local.children.iter().any(|child| child.name == name)
        || local
            .uses
            .iter()
            .any(|leaf| leaf.bound_name() == Some(name))
}

/// `start` followed along `segments`: `crate` and `self` stay, `super` climbs, a name descends.
fn walked(mut path: Vec<String>, segments: &[String]) -> Option<Vec<String>> {
    for segment in segments {
        match segment.as_str() {
            "crate" | "self" => {}
            "super" => {
                path.pop()?;
            }
            name => path.push(name.to_string()),
        }
    }
    Some(path)
}

/// A package on disk, for the tests of the backend's lexical readings.
#[cfg(test)]
pub(in crate::backends::rust) mod on_disk {
    use crate::overlay::Overlay;
    use crate::registry::Workspace;

    /// One package, `app`, holding the files a test writes (paths relative to the package root).
    pub(in crate::backends::rust) struct AnAppOnDisk {
        directory: tempfile::TempDir,
    }

    /// An `app` package holding exactly `files`, beside its manifest.
    pub(in crate::backends::rust) fn an_app_holding(files: &[(&str, &str)]) -> AnAppOnDisk {
        let app = AnAppOnDisk {
            directory: tempfile::tempdir().expect("a temporary directory"),
        };
        app.write(
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        for (path, text) in files {
            app.write(path, text);
        }
        app
    }

    impl AnAppOnDisk {
        fn write(&self, relative: &str, text: &str) {
            let absolute = self.directory.path().join(relative);
            std::fs::create_dir_all(absolute.parent().expect("a parent directory"))
                .expect("the directory is created");
            std::fs::write(absolute, text).expect("the file is written");
        }

        /// Run `read` over the package as a workspace with nothing staged.
        pub(in crate::backends::rust) fn reading<T>(
            &self,
            read: impl FnOnce(&Workspace<'_>) -> T,
        ) -> T {
            let overlay = Overlay::new();
            let workspace = Workspace {
                root: self.directory.path(),
                overlay: &overlay,
            };
            read(&workspace)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crate_move::source_scan::items_of_module;

    fn path(text: &str) -> Vec<String> {
        text.split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// What `written` names, written in a `use` of the module `at` whose top level is `module_text`.
    fn reading(written: &str, at: &str, module_text: &str) -> Option<UsePath> {
        resolved(&path(written), &path(at), &items_of_module(module_text))
    }

    #[test]
    fn a_head_the_module_does_not_bind_is_an_extern_crate() {
        // Given a module that binds nothing named `kernel`
        let module = "pub use kernel::util;\nmod observer;\n";

        // When the facade's path is read
        let named = reading("kernel::util", "host", module);

        // Then it names the crate `kernel`, not a child of `host`
        assert_eq!(named, Some(UsePath::Extern(path("kernel::util"))));
    }

    #[test]
    fn a_head_naming_a_child_module_an_item_or_an_import_of_the_module_is_local() {
        // Given a module that declares `inner`, defines `Mode` and imports a module as `alias`
        let module = "mod inner;\nenum Mode { Fast }\nuse crate::x as alias;\n";

        // When a path headed by each is read
        let child = reading("inner::X", "host", module);
        let variant = reading("Mode::Fast", "host", module);
        let imported = reading("alias::Y", "host", module);

        // Then each is a path of this crate below `host`
        assert_eq!(child, Some(UsePath::InCrate(path("host::inner::X"))));
        assert_eq!(variant, Some(UsePath::InCrate(path("host::Mode::Fast"))));
        assert_eq!(imported, Some(UsePath::InCrate(path("host::alias::Y"))));
    }

    #[test]
    fn crate_self_and_super_heads_resolve_against_the_module_and_climbing_above_the_root_is_none() {
        // Given a module two levels below the root
        let (at, module) = ("svc::exec", "");

        // When paths headed by `crate`, `self`, `super` and three `super`s are read
        let rooted = reading("crate::types::Config", at, module);
        let own = reading("self::guard::Route", at, module);
        let parent = reading("super::guard::Route", at, module);
        let above = reading("super::super::super::x", at, module);

        // Then each is resolved against `svc::exec`, and the one that climbs above the root is none
        assert_eq!(rooted, Some(UsePath::InCrate(path("types::Config"))));
        assert_eq!(own, Some(UsePath::InCrate(path("svc::exec::guard::Route"))));
        assert_eq!(parent, Some(UsePath::InCrate(path("svc::guard::Route"))));
        assert_eq!(above, None);
    }
}
