//! The workspace's crate dependency graph, as Cargo resolves it.
//!
//! Read from `cargo metadata` rather than by parsing manifests, so a dependency is seen however its
//! manifest spells it — inline, as a `[dependencies.name]` sub-table, or inherited with
//! `{ workspace = true }`. Two hand-rolled manifest walkers preceded this and each missed the last
//! two forms.
//!
//! "Names" means a `[dependencies]` or `[build-dependencies]` entry, optional ones included: an
//! optional dependency is still one the crate can be built with. `[dev-dependencies]` are not
//! followed — a test may reach anything.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

/// The crates of one Cargo workspace and the dependencies each one names.
#[derive(Debug, Clone)]
pub struct WorkspaceGraph {
    /// Each workspace member and the crates it names in `[dependencies]` / `[build-dependencies]`.
    names: BTreeMap<String, BTreeSet<String>>,
    /// Each workspace member and the names of its `bin` targets.
    binaries: BTreeMap<String, BTreeSet<String>>,
}

impl WorkspaceGraph {
    /// The workspace this checkout's crates belong to.
    pub fn load() -> Self {
        Self::read(Path::new(env!("CARGO_MANIFEST_DIR")))
    }

    /// The workspace whose root manifest is `workspace_root/Cargo.toml`.
    pub fn of_workspace(workspace_root: &Path) -> Self {
        Self::read(workspace_root)
    }

    /// Every crate that is a member of the workspace.
    pub fn workspace_crates(&self) -> BTreeSet<String> {
        self.names.keys().cloned().collect()
    }

    /// The crates workspace member `krate` names directly, workspace members and registry crates
    /// alike.
    ///
    /// # Panics
    ///
    /// When `krate` is not a member of the workspace: a misspelled name must not read as a crate
    /// that names nothing, or every "does not name X" check on it would pass vacuously.
    pub fn names_directly(&self, krate: &str) -> BTreeSet<String> {
        self.names.get(krate).cloned().unwrap_or_else(|| {
            panic!(
                "`{krate}` is not a member of the workspace (members: {:?})",
                self.names.keys().collect::<Vec<_>>()
            )
        })
    }

    /// Every crate reachable from `krate` by following what each crate names, `krate` excluded.
    /// Registry crates are reached but not followed further.
    pub fn reaches(&self, krate: &str) -> BTreeSet<String> {
        let mut reached = BTreeSet::new();
        let mut pending = vec![krate.to_string()];
        while let Some(current) = pending.pop() {
            // A registry crate is not a member, so it names nothing here: reached, not followed.
            let named_by_current = self.names.get(&current).cloned().unwrap_or_default();
            for named in named_by_current {
                if named != krate && reached.insert(named.clone()) {
                    pending.push(named);
                }
            }
        }
        reached
    }

    /// The workspace crates with a `bin` target named `binary`.
    ///
    /// Empty when no member builds it. A caller compares the answer with the crate it expects, so a
    /// misspelled binary fails there rather than reading as "built by nothing".
    pub fn crates_building(&self, binary: &str) -> BTreeSet<String> {
        self.binaries
            .iter()
            .filter(|(_, binaries)| binaries.contains(binary))
            .map(|(krate, _)| krate.clone())
            .collect()
    }

    /// Run `cargo metadata` in `directory` — the one place the graph is read from.
    fn read(directory: &Path) -> Self {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let output = Command::new(cargo)
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(directory)
            .output()
            .expect("run `cargo metadata`");
        assert!(
            output.status.success(),
            "`cargo metadata` failed in {}: {}",
            directory.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("`cargo metadata` prints JSON");

        let packages = metadata["packages"]
            .as_array()
            .expect("metadata lists packages");
        let names = packages
            .iter()
            .map(|package| {
                let name = package["name"].as_str().expect("a package has a name");
                let dependencies = package["dependencies"]
                    .as_array()
                    .expect("a package lists its dependencies")
                    .iter()
                    // `kind` is null for `[dependencies]`, "build" or "dev" otherwise.
                    .filter(|dependency| dependency["kind"].as_str() != Some("dev"))
                    .map(|dependency| {
                        dependency["name"]
                            .as_str()
                            .expect("a dependency has a name")
                            .to_string()
                    })
                    .collect();
                (name.to_string(), dependencies)
            })
            .collect();
        let binaries = packages
            .iter()
            .map(|package| {
                let name = package["name"].as_str().expect("a package has a name");
                let bins = package["targets"]
                    .as_array()
                    .expect("a package lists its targets")
                    .iter()
                    .filter(|target| {
                        target["kind"]
                            .as_array()
                            .expect("a target lists its kinds")
                            .iter()
                            .any(|kind| kind.as_str() == Some("bin"))
                    })
                    .map(|target| {
                        target["name"]
                            .as_str()
                            .expect("a target has a name")
                            .to_string()
                    })
                    .collect();
                (name.to_string(), bins)
            })
            .collect();
        Self { names, binaries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A four-crate workspace spelling its dependencies every way a manifest can:
    ///
    /// - `app` names `core` inline, `store` as a `[dependencies.store]` sub-table, `serde` as an
    ///   optional registry dependency, `probe` only under `[dev-dependencies]`, and `codegen` under
    ///   `[build-dependencies]`;
    /// - `store` names `core` through `{ workspace = true }`.
    fn a_workspace_spelling_dependencies_every_way() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        let write = |path: &str, contents: &str| {
            let file = root.path().join(path);
            std::fs::create_dir_all(file.parent().expect("a file has a parent"))
                .expect("create the crate directory");
            std::fs::write(file, contents).expect("write the fixture file");
        };
        write(
            "Cargo.toml",
            "[workspace]\nmembers = [\"app\", \"store\", \"core\", \"probe\", \"codegen\"]\n\
             resolver = \"2\"\n\n[workspace.dependencies]\ncore = { path = \"core\" }\n",
        );
        write(
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\ncore = { path = \"../core\" }\n\
             serde = { version = \"1\", optional = true }\n\n\
             [dependencies.store]\npath = \"../store\"\n\n\
             [dev-dependencies]\nprobe = { path = \"../probe\" }\n\n\
             [build-dependencies]\ncodegen = { path = \"../codegen\" }\n",
        );
        write(
            "store/Cargo.toml",
            "[package]\nname = \"store\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\ncore = { workspace = true }\n",
        );
        for leaf in ["core", "probe", "codegen"] {
            write(
                &format!("{leaf}/Cargo.toml"),
                &format!("[package]\nname = \"{leaf}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
            );
        }
        for krate in ["app", "store", "core", "probe", "codegen"] {
            write(&format!("{krate}/src/lib.rs"), "");
        }
        root
    }

    fn names(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn lists_every_member_of_the_workspace() {
        let root = a_workspace_spelling_dependencies_every_way();

        let graph = WorkspaceGraph::of_workspace(root.path());

        assert_eq!(
            graph.workspace_crates(),
            names(&["app", "codegen", "core", "probe", "store"])
        );
    }

    #[test]
    fn names_inline_sub_table_optional_and_build_dependencies_but_not_dev_ones() {
        // Given a crate declaring its dependencies in every form
        let root = a_workspace_spelling_dependencies_every_way();

        // When what it names is read
        let graph = WorkspaceGraph::of_workspace(root.path());

        // Then a sub-table, an optional registry crate and a build dependency all count; a test-only
        // one does not
        assert_eq!(
            graph.names_directly("app"),
            names(&["codegen", "core", "serde", "store"])
        );
    }

    #[test]
    fn names_a_dependency_inherited_from_the_workspace() {
        let root = a_workspace_spelling_dependencies_every_way();

        let graph = WorkspaceGraph::of_workspace(root.path());

        assert_eq!(graph.names_directly("store"), names(&["core"]));
    }

    /// A workspace of `members`, each a crate name with the body of its manifest after the
    /// `[package]` table (its `[dependencies]`, `[dev-dependencies]`, ...).
    fn a_workspace_of(members: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        let listed: Vec<String> = members
            .iter()
            .map(|(name, _)| format!("\"{name}\""))
            .collect();
        std::fs::write(
            root.path().join("Cargo.toml"),
            format!(
                "[workspace]\nmembers = [{}]\nresolver = \"2\"\n",
                listed.join(", ")
            ),
        )
        .expect("write the workspace manifest");
        for (name, body) in members {
            let source = root.path().join(name).join("src");
            std::fs::create_dir_all(&source).expect("create the crate directory");
            std::fs::write(source.join("lib.rs"), "").expect("write lib.rs");
            std::fs::write(
                root.path().join(name).join("Cargo.toml"),
                format!(
                    "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n{body}"
                ),
            )
            .expect("write the crate manifest");
        }
        root
    }

    #[test]
    fn reaches_a_crate_only_another_crate_names() {
        // Given `app` naming `middle`, which alone names `leaf`
        let root = a_workspace_of(&[
            ("app", "[dependencies]\nmiddle = { path = \"../middle\" }\n"),
            ("middle", "[dependencies]\nleaf = { path = \"../leaf\" }\n"),
            ("leaf", ""),
        ]);

        // When what `app` reaches is read
        let reached = WorkspaceGraph::of_workspace(root.path()).reaches("app");

        // Then the second hop is reached
        assert_eq!(reached, names(&["leaf", "middle"]));
    }

    #[test]
    fn reaches_a_registry_crate_without_following_it() {
        // Given `app` naming a registry crate
        let root = a_workspace_of(&[("app", "[dependencies]\nserde = \"1\"\n")]);

        // When what `app` reaches is read
        let reached = WorkspaceGraph::of_workspace(root.path()).reaches("app");

        // Then the registry crate is reached
        assert_eq!(reached, names(&["serde"]));
    }

    #[test]
    fn never_reaches_the_crate_it_started_from_even_when_a_dependency_names_it_back() {
        // Given `leaf` naming `app` back, as a build dependency
        let root = a_workspace_of(&[
            ("app", "[dependencies]\nleaf = { path = \"../leaf\" }\n"),
            (
                "leaf",
                "[build-dependencies]\napp = { path = \"../app\" }\n",
            ),
        ]);

        // When what `app` reaches is read
        let reached = WorkspaceGraph::of_workspace(root.path()).reaches("app");

        // Then `app` is not among it
        assert_eq!(reached, names(&["leaf"]));
    }

    #[test]
    fn does_not_follow_a_dev_dependency_to_what_only_it_names() {
        // Given `app` naming `probe` only under `[dev-dependencies]`, and `probe` naming `deep`
        let root = a_workspace_of(&[
            (
                "app",
                "[dev-dependencies]\nprobe = { path = \"../probe\" }\n",
            ),
            ("probe", "[dependencies]\ndeep = { path = \"../deep\" }\n"),
            ("deep", ""),
        ]);

        // When what `app` reaches is read
        let reached = WorkspaceGraph::of_workspace(root.path()).reaches("app");

        // Then neither is reached
        assert_eq!(reached, names(&[]));
    }

    #[test]
    fn a_binary_is_built_by_the_crate_with_that_bin_target_not_by_a_library_of_the_same_name() {
        // Given a library `app` and a crate `app-cli` whose binary is also called `app`
        let root = a_workspace_of(&[
            ("app", ""),
            (
                "app-cli",
                "[[bin]]\nname = \"app\"\npath = \"src/main.rs\"\n",
            ),
        ]);
        std::fs::write(root.path().join("app-cli/src/main.rs"), "fn main() {}\n")
            .expect("write main.rs");

        // When the crates building `app` are read
        let building = WorkspaceGraph::of_workspace(root.path()).crates_building("app");

        // Then only the crate with the bin target is: a lib target of the same name does not count
        assert_eq!(building, names(&["app-cli"]));
    }

    #[test]
    #[should_panic(expected = "`tddy-no-such-crate` is not a member of the workspace")]
    fn a_crate_that_is_not_a_member_cannot_be_asked_what_it_names() {
        // Given a workspace without the crate asked about
        let root = a_workspace_of(&[("app", "")]);

        // When what a misspelled crate names is read
        WorkspaceGraph::of_workspace(root.path()).names_directly("tddy-no-such-crate");

        // Then it panics rather than answering "nothing"
    }
}
