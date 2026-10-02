//! Following a path through the origin's re-exports to the place it is defined.
//!
//! [`module_home::defining_crate`] resolves **one** hop, at the origin's crate root, and answers
//! which crate a module belongs to. A moved file's paths need more than that: `super::helper` from a
//! nested module reaches a name the parent module only *globs in* from another crate, and
//! `crate::roster` reaches a module the origin fills with nothing but a glob. Both have to be
//! followed to the item and the crate that define it, because that is what the path has to be
//! re-pointed at.
//!
//! The walk reads sources and nothing else: a `mod` block or file, the `use` items at a module's top
//! level, the items it defines. A name is looked for in the order Rust resolves it — a child module,
//! an item defined in the module, an explicit `use`, then the module's globs, each of which has to
//! *confirm* the name before it counts. A crate this workspace does not reach by a path dependency
//! cannot confirm one, so a glob from it is not followed; an explicit `use` of it is, because the
//! source says so outright.

use std::collections::BTreeSet;

use super::destination::Destination;
use super::source_scan::{items_of_module, ChildModule, ModuleItems};
use super::survey::resolved_against;
use super::Result;
use crate::registry::Workspace;

/// The crate-rooted path `path` is defined at, once every re-export on the way is followed.
///
/// `origin::outer::helper` is returned as `destination::helper_mod::helper` when `outer` globs
/// `destination::helper_mod`. A path the origin defines itself, and one this walk cannot see
/// further into, come back as written. A path that does not begin with the origin's extern name is
/// left alone: its meaning does not change when a file leaves the origin.
///
/// # Errors
///
/// Refuses when the origin's crate root cannot be read, or a `super::` in a source climbs above the
/// crate root.
pub(crate) fn followed(
    workspace: &Workspace<'_>,
    origin: &Destination,
    path: &str,
) -> Result<String> {
    let segments: Vec<String> = path.split("::").map(str::to_string).collect();
    if segments.first() != Some(&origin.extern_name) {
        return Ok(path.to_string());
    }

    let mut walk = Walk {
        workspace,
        visited: BTreeSet::new(),
    };
    Ok(walk
        .follow_absolute(origin, &segments)?
        .unwrap_or_else(|| path.to_string()))
}

/// A module's text, and where the files of its own child modules live.
struct ModuleSource {
    text: String,
    /// The directory a `mod name;` inside it is looked for in, relative to the repository root.
    dir: String,
    /// The module path inside its crate, outermost first.
    path: Vec<String>,
}

struct Walk<'a, 'w> {
    workspace: &'a Workspace<'w>,
    /// Every absolute path already followed, so a pair of modules re-exporting each other ends.
    visited: BTreeSet<String>,
}

impl Walk<'_, '_> {
    /// Follow a crate-rooted path whose first segment is a crate: `krate`'s own, or one `krate`
    /// reaches by a path dependency.
    ///
    /// `None` when the path cannot be confirmed: its crate is not readable here, or no module on the
    /// way holds the name.
    fn follow_absolute(&mut self, krate: &Destination, full: &[String]) -> Result<Option<String>> {
        let Some((head, rest)) = full.split_first() else {
            return Ok(None);
        };
        if !self.visited.insert(full.join("::")) {
            return Ok(None);
        }

        let target = if *head == krate.extern_name {
            krate.clone()
        } else {
            match krate.path_dependency(self.workspace.root, head)? {
                Some(dependency) => dependency,
                None => return Ok(None),
            }
        };
        let root = ModuleSource {
            text: self.workspace.read(&format!("{}/src/lib.rs", target.dir))?,
            dir: format!("{}/src", target.dir),
            path: Vec::new(),
        };
        self.walk(&target, &root, rest)
    }

    fn walk(
        &mut self,
        krate: &Destination,
        module: &ModuleSource,
        rest: &[String],
    ) -> Result<Option<String>> {
        let items = items_of_module(&module.text);
        let Some((name, tail)) = rest.split_first() else {
            return self.address_of(krate, module, &items);
        };

        if let Some(child) = items.children.iter().find(|child| child.name == *name) {
            return match self.child_source(module, child) {
                Some(source) => self.walk(krate, &source, tail),
                None => Ok(None),
            };
        }
        if items.defined.contains(name) {
            return Ok(Some(address(krate, &module.path, rest)));
        }

        for leaf in items.uses.iter().filter(|leaf| !leaf.glob) {
            if leaf.bound_name() != Some(name.as_str()) {
                continue;
            }
            let mut target = self.absolute(krate, module, &items.children, &leaf.segments)?;
            target.extend(tail.iter().cloned());
            let followed = self.follow_absolute(krate, &target)?;
            return Ok(Some(followed.unwrap_or_else(|| target.join("::"))));
        }

        for leaf in items.uses.iter().filter(|leaf| leaf.glob) {
            let mut candidate = self.absolute(krate, module, &items.children, &leaf.segments)?;
            candidate.extend(rest.iter().cloned());
            if let Some(found) = self.follow_absolute(krate, &candidate)? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    /// The path of the module itself.
    ///
    /// A module that is nothing but one glob re-export *is* what it globs, so it is followed there:
    /// `pub mod roster { pub use destination::records::*; }` is `destination::records`. A module that
    /// holds anything else of its own is the crate's, and stays where it is.
    fn address_of(
        &mut self,
        krate: &Destination,
        module: &ModuleSource,
        items: &ModuleItems,
    ) -> Result<Option<String>> {
        let nothing_else = items.children.is_empty() && items.defined.is_empty();
        if let ([only], true) = (items.uses.as_slice(), nothing_else) {
            if only.glob {
                let target = self.absolute(krate, module, &[], &only.segments)?;
                let followed = self.follow_absolute(krate, &target)?;
                return Ok(Some(followed.unwrap_or_else(|| target.join("::"))));
            }
        }
        Ok(Some(address(krate, &module.path, &[])))
    }

    /// A `use` path from inside `module`, written from the crate root by extern name.
    ///
    /// `crate`, `self` and `super` are resolved against the module's own path; a first segment that
    /// names a child module is that module, which is what Rust 2018 reads it as; anything else
    /// already names a crate.
    fn absolute(
        &self,
        krate: &Destination,
        module: &ModuleSource,
        children: &[ChildModule],
        segments: &[String],
    ) -> Result<Vec<String>> {
        let Some(head) = segments.first() else {
            return Ok(Vec::new());
        };
        let written = segments.join("::");
        let resolved = if matches!(head.as_str(), "crate" | "self" | "super") {
            resolved_against(&written, &krate.extern_name, &module.path)?
        } else if children.iter().any(|child| child.name == *head) {
            resolved_against(
                &format!("self::{written}"),
                &krate.extern_name,
                &module.path,
            )?
        } else {
            written
        };
        Ok(resolved.split("::").map(str::to_string).collect())
    }

    /// A child module's own source, or `None` when no file for it can be read.
    fn child_source(&self, parent: &ModuleSource, child: &ChildModule) -> Option<ModuleSource> {
        let dir = format!("{}/{}", parent.dir, child.name);
        let path = parent
            .path
            .iter()
            .cloned()
            .chain(std::iter::once(child.name.clone()))
            .collect();

        let text = match &child.body {
            Some(body) => parent.text[body.clone()].to_string(),
            None => self
                .workspace
                .read(&format!("{}/{}.rs", parent.dir, child.name))
                .or_else(|_| self.workspace.read(&format!("{dir}/mod.rs")))
                .ok()?,
        };
        Some(ModuleSource { text, dir, path })
    }
}

/// `krate::module::rest`, by the crate's extern name.
fn address(krate: &Destination, module: &[String], rest: &[String]) -> String {
    std::iter::once(krate.extern_name.as_str())
        .chain(module.iter().map(String::as_str))
        .chain(rest.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("::")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::Overlay;

    /// A workspace of crates on disk, each with a manifest and the files a test writes.
    struct AWorkspaceOnDisk {
        directory: tempfile::TempDir,
    }

    fn a_workspace_of_origin_over_destination(
        origin_lib: &str,
        destination_lib: &str,
    ) -> AWorkspaceOnDisk {
        let workspace = AWorkspaceOnDisk {
            directory: tempfile::tempdir().expect("a temporary directory"),
        };
        workspace
            .writing(
                "origin/Cargo.toml",
                "[package]\nname = \"origin\"\n\n[dependencies]\ndestination = { path = \"../destination\" }\n",
            )
            .writing("origin/src/lib.rs", origin_lib)
            .writing("destination/Cargo.toml", "[package]\nname = \"destination\"\n")
            .writing("destination/src/lib.rs", destination_lib)
    }

    impl AWorkspaceOnDisk {
        fn writing(self, relative: &str, text: &str) -> Self {
            let absolute = self.directory.path().join(relative);
            std::fs::create_dir_all(absolute.parent().expect("a parent directory"))
                .expect("the directory is created");
            std::fs::write(absolute, text).expect("the file is written");
            self
        }

        /// Where `path` is defined once the origin's re-exports are followed.
        fn defining(&self, path: &str) -> String {
            let overlay = Overlay::new();
            let workspace = Workspace {
                root: self.directory.path(),
                overlay: &overlay,
            };
            let origin = Destination::read(self.directory.path(), "origin")
                .expect("the origin has a manifest");

            followed(&workspace, &origin, path).expect("the sources read")
        }
    }

    #[test]
    fn an_explicit_re_export_is_followed_into_the_crate_that_defines_the_item() {
        // Given an origin forwarding a module of the destination
        let workspace = a_workspace_of_origin_over_destination(
            "pub use destination::config;\n",
            "pub mod config {\n    pub struct Settings;\n}\n",
        );

        // When a path through the forwarded module is followed
        let defined_at = workspace.defining("origin::config::Settings");

        // Then it is the destination's item
        assert_eq!(defined_at, "destination::config::Settings");
    }

    #[test]
    fn a_glob_re_export_counts_only_for_a_name_the_target_defines() {
        // Given a module globbing the destination, which defines one of the two names asked for
        let workspace = a_workspace_of_origin_over_destination(
            "pub mod outer {\n    pub use destination::helpers::*;\n}\n",
            "pub mod helpers {\n    pub fn present() {}\n}\n",
        );

        // When both are followed
        let present = workspace.defining("origin::outer::present");
        let absent = workspace.defining("origin::outer::absent");

        // Then the defined one moves to the destination and the other stays where it was written
        assert_eq!(present, "destination::helpers::present");
        assert_eq!(absent, "origin::outer::absent");
    }

    #[test]
    fn an_item_the_origin_defines_beside_a_glob_stays_the_origins() {
        // Given a module that defines `helper` itself and also globs a module defining it
        let workspace = a_workspace_of_origin_over_destination(
            "pub mod outer {\n    pub use destination::helpers::*;\n    pub fn helper() {}\n}\n",
            "pub mod helpers {\n    pub fn helper() {}\n}\n",
        );

        // When it is followed
        let defined_at = workspace.defining("origin::outer::helper");

        // Then the explicit definition wins over the glob
        assert_eq!(defined_at, "origin::outer::helper");
    }

    #[test]
    fn a_module_that_is_one_glob_re_export_is_the_module_it_globs() {
        // Given a module holding nothing but a glob of a destination module
        let workspace = a_workspace_of_origin_over_destination(
            "pub mod roster {\n    pub use destination::records::*;\n}\n",
            "pub mod records {\n    pub fn id() {}\n}\n",
        );

        // When the module itself is followed
        let defined_at = workspace.defining("origin::roster");

        // Then it is the destination's module
        assert_eq!(defined_at, "destination::records");
    }

    #[test]
    fn two_modules_re_exporting_each_other_end_the_walk_where_it_began() {
        // Given globs that point at each other, each crate depending on the other, defining nothing
        let workspace =
            a_workspace_of_origin_over_destination("pub use destination::*;\n", "pub use origin::*;\n")
                .writing(
                    "destination/Cargo.toml",
                    "[package]\nname = \"destination\"\n\n[dependencies]\norigin = { path = \"../origin\" }\n",
                );

        // When a name neither defines is followed
        let defined_at = workspace.defining("origin::missing");

        // Then the path is returned as written
        assert_eq!(defined_at, "origin::missing");
    }

    #[test]
    fn a_path_naming_another_crate_is_left_alone() {
        // Given any origin
        let workspace = a_workspace_of_origin_over_destination("", "");

        // When a path rooted in another crate is followed
        let defined_at = workspace.defining("destination::config::Settings");

        // Then it is returned as written
        assert_eq!(defined_at, "destination::config::Settings");
    }
}
