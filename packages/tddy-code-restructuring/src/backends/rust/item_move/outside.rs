//! `reexport: outside`: which callers lie outside the crate that holds the moved code.
//!
//! The one decision both `move_item` and `reparent_module` take, so they share it. A site is outside
//! when the package that owns its file is not the package that owns the moved code — read from the
//! manifests, the way [`package_of`] reads every package question here, and not guessed from a
//! visibility keyword — and, within the package, by the target the file belongs to: only the library
//! crate (the files under `src/`) is inside. An integration test, example or bench is a crate of its
//! own that reaches the package through its public paths, and so is the `src/main.rs` or `src/bin/`
//! of a package that also has a `src/lib.rs`. (A custom `[lib] path` or `[[bin]] path` is not read: the
//! default layout is assumed.) The sites inside are re-pointed like `none` re-points them; the names the
//! outside sites reach are the only ones a facade is left for; the outside sites themselves are never
//! edited.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::destination::{package_of, Package};
use super::scope::Scope;
use super::sites::Site;
use crate::plan::Reexport;
use crate::Result;

/// The callers of a move, split for its `reexport`.
pub(in crate::backends::rust) struct Reach {
    /// The sites the move re-points or measures: all of them, except that under `outside` the ones in
    /// another package are left out, because they are not edited.
    pub(in crate::backends::rust) sites: Vec<Site>,
    /// The moved names something in another package reaches. Empty unless the move is `outside`.
    pub(in crate::backends::rust) outside: BTreeSet<String>,
}

impl Reach {
    /// Split `sites` by the package that owns each file, when the move is `outside`; otherwise leave
    /// them whole.
    pub(in crate::backends::rust) fn of(
        reexport: Reexport,
        root: &Path,
        package: &Package,
        sites: Vec<Site>,
    ) -> Result<Reach> {
        if reexport != Reexport::Outside {
            return Ok(Reach {
                sites,
                outside: BTreeSet::new(),
            });
        }
        let mut owners: BTreeMap<String, bool> = BTreeMap::new();
        let mut inside = Vec::new();
        let mut outside = BTreeSet::new();
        for site in sites {
            if !owners.contains_key(&site.path) {
                let owned = lies_in_the_library(root, &site.path, package)?;
                owners.insert(site.path.clone(), owned);
            }
            if owners[&site.path] {
                inside.push(site);
            } else {
                outside.insert(site.name);
            }
        }
        Ok(Reach {
            sites: inside,
            outside,
        })
    }
}

/// Whether `file` is part of the library crate of `package`: in the package, under its `src/`, and not
/// the root or a file of a binary target that sits beside a library.
fn lies_in_the_library(root: &Path, file: &str, package: &Package) -> Result<bool> {
    if package_of(root, file)?.dir != package.dir {
        return Ok(false);
    }
    let source = package.dir.join("src");
    let file = Path::new(file);
    if !file.starts_with(&source) {
        return Ok(false);
    }
    let has_library = root.join(&source).join("lib.rs").is_file();
    let is_binary = file == source.join("main.rs") || file.starts_with(source.join("bin"));
    Ok(!(has_library && is_binary))
}

/// The items a facade is left for: every one under `glob` and `named`, and under `outside` only the
/// ones something outside the crate reaches.
pub(in crate::backends::rust) fn facade_items(
    reexport: Reexport,
    items: &[(String, Scope)],
    outside: &BTreeSet<String>,
) -> Vec<(String, Scope)> {
    items
        .iter()
        .filter(|(name, _)| reexport != Reexport::Outside || outside.contains(name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn a_package_with(files: &[&str]) -> (tempfile::TempDir, Package) {
        let root = tempfile::tempdir().expect("a root");
        let mut all = vec!["Cargo.toml"];
        all.extend_from_slice(files);
        for file in all {
            let path = root.path().join("app").join(file);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            let text = if file == "Cargo.toml" {
                "[package]\nname = \"app\"\n"
            } else {
                ""
            };
            std::fs::write(path, text).expect("a file");
        }
        let package = Package {
            dir: PathBuf::from("app"),
            crate_name: "app".to_string(),
        };
        (root, package)
    }

    #[test]
    fn counts_only_the_library_files_of_a_package_as_inside() {
        let (root, package) = a_package_with(&["src/lib.rs", "src/main.rs", "src/bin/tool.rs"]);
        let inside =
            |file: &str| lies_in_the_library(root.path(), file, &package).expect("a verdict");
        assert_eq!(
            [
                inside("app/src/lib.rs"),
                inside("app/src/main.rs"),
                inside("app/src/bin/tool.rs"),
                inside("app/tests/a.rs"),
                inside("app/examples/a.rs"),
                inside("app/benches/a.rs"),
            ],
            [true, false, false, false, false, false]
        );
    }

    #[test]
    fn counts_the_main_file_of_a_binary_only_package_as_inside() {
        let (root, package) = a_package_with(&["src/main.rs"]);
        assert!(lies_in_the_library(root.path(), "app/src/main.rs", &package).expect("a verdict"));
    }

    fn items() -> Vec<(String, Scope)> {
        ["a", "b"]
            .iter()
            .map(|name| (name.to_string(), Scope::Public))
            .collect()
    }

    #[test]
    fn keeps_every_item_unless_the_move_is_outside() {
        assert_eq!(
            facade_items(Reexport::Named, &items(), &BTreeSet::new()).len(),
            2
        );
    }

    #[test]
    fn keeps_only_the_items_something_outside_reaches() {
        let reached = BTreeSet::from(["b".to_string()]);
        let kept = facade_items(Reexport::Outside, &items(), &reached);
        assert_eq!(
            kept.iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["b"]
        );
    }

    #[test]
    fn keeps_nothing_when_nothing_outside_reaches() {
        assert!(facade_items(Reexport::Outside, &items(), &BTreeSet::new()).is_empty());
    }
}
