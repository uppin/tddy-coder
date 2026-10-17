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

    /// The crates `krate` names directly, workspace members and registry crates alike.
    pub fn names_directly(&self, krate: &str) -> BTreeSet<String> {
        self.names.get(krate).cloned().unwrap_or_default()
    }

    /// Every crate reachable from `krate` by following what each crate names, `krate` excluded.
    /// Registry crates are reached but not followed further.
    pub fn reaches(&self, krate: &str) -> BTreeSet<String> {
        let mut reached = BTreeSet::new();
        let mut pending = vec![krate.to_string()];
        while let Some(current) = pending.pop() {
            for named in self.names_directly(&current) {
                if named != krate && reached.insert(named.clone()) {
                    pending.push(named);
                }
            }
        }
        reached
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
        Self { names }
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

    #[test]
    fn reaches_every_hop_but_not_the_crate_itself_nor_its_dev_dependencies() {
        // Given a crate whose dependency has a dependency of its own
        let root = a_workspace_spelling_dependencies_every_way();

        // When everything it can be built with is followed
        let graph = WorkspaceGraph::of_workspace(root.path());

        // Then the second hop is reached, a registry crate is reached, and a dev-dependency is not
        assert_eq!(
            graph.reaches("app"),
            names(&["codegen", "core", "serde", "store"])
        );
    }
}
