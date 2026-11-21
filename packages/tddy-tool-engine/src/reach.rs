//! An orchestrator's read reach (`#orchestra` 1/9): the roots its `Read`, `Grep` and `Glob` may
//! answer from. The daemon builds a [`ReadReach`] per call from every tddy worktree on the host, the
//! coding agents' declared home directories and the session's grants; this module only applies it.
//!
//! A relative path keeps meaning the caller's own home worktree. An absolute path is answered when
//! it lies inside a root, after symlinks are resolved; anything else is refused with a message that
//! names `RequestPathAccess`, the tool that asks the operator for more.

use std::path::{Path, PathBuf};

use crate::ToolOutcome;

/// The tool an orchestrator asks the operator for a path outside its reach with.
pub const REQUEST_PATH_ACCESS_TOOL: &str = "RequestPathAccess";

/// The tools answered through a [`ReadReach`] rather than a single worktree root.
pub fn is_reach_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Read" | "Grep" | "Glob")
}

/// The roots one orchestrator call may read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadReach {
    home: PathBuf,
    roots: Vec<PathBuf>,
}

impl ReadReach {
    /// A reach whose relative paths resolve under `home` (the caller's home worktree, always in
    /// reach) and whose absolute paths may lie under `home` or any of `roots`.
    pub fn new(home: PathBuf, roots: Vec<PathBuf>) -> Self {
        Self { home, roots }
    }

    /// The caller's home worktree.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Every root besides the home, in the order given.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Whether `path` (absolute, symlinks resolved) lies under the home or a root.
    pub fn contains(&self, path: &Path) -> bool {
        // TODO(path-reach): implement
        let _ = path;
        unimplemented!("ReadReach::contains")
    }

    /// Resolve a tool's path argument: relative under the home, absolute as given, then canonical
    /// and inside reach — or a refusal naming [`REQUEST_PATH_ACCESS_TOOL`].
    pub fn resolve(&self, arg_path: &str) -> Result<PathBuf, String> {
        // TODO(path-reach): implement
        let _ = arg_path;
        unimplemented!("ReadReach::resolve")
    }
}

/// Run `Read`, `Grep` or `Glob` within `reach`. Any other tool is an error outcome.
pub async fn execute_in_reach(reach: &ReadReach, tool_name: &str, args_json: &str) -> ToolOutcome {
    // TODO(path-reach): implement
    let _ = (reach, tool_name, args_json);
    unimplemented!("execute_in_reach")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World {
        home: tempfile::TempDir,
        other: tempfile::TempDir,
        outside: tempfile::TempDir,
    }

    impl World {
        fn new() -> Self {
            let world = Self {
                home: tempfile::tempdir().unwrap(),
                other: tempfile::tempdir().unwrap(),
                outside: tempfile::tempdir().unwrap(),
            };
            std::fs::write(world.home.path().join("own.txt"), "own\n").unwrap();
            std::fs::write(world.other.path().join("theirs.txt"), "theirs\n").unwrap();
            std::fs::write(world.outside.path().join("secret.txt"), "secret\n").unwrap();
            world
        }

        fn reach(&self) -> ReadReach {
            ReadReach::new(
                self.home.path().canonicalize().unwrap(),
                vec![self.other.path().canonicalize().unwrap()],
            )
        }
    }

    #[test]
    fn a_relative_path_resolves_under_the_home() {
        let world = World::new();

        let resolved = world.reach().resolve("own.txt");

        assert_eq!(
            resolved,
            Ok(world.home.path().canonicalize().unwrap().join("own.txt"))
        );
    }

    #[test]
    fn an_absolute_path_inside_a_root_resolves() {
        let world = World::new();
        let theirs = world
            .other
            .path()
            .canonicalize()
            .unwrap()
            .join("theirs.txt");

        let resolved = world.reach().resolve(theirs.to_str().unwrap());

        assert_eq!(resolved, Ok(theirs));
    }

    #[test]
    fn an_absolute_path_outside_reach_is_refused_naming_request_path_access() {
        let world = World::new();
        let secret = world.outside.path().join("secret.txt");

        let refusal = world.reach().resolve(secret.to_str().unwrap()).unwrap_err();

        assert!(refusal.contains(REQUEST_PATH_ACCESS_TOOL), "{refusal}");
    }

    #[test]
    fn a_symlink_inside_reach_pointing_outside_it_is_refused() {
        let world = World::new();
        let link = world.home.path().join("escape");
        std::os::unix::fs::symlink(world.outside.path().join("secret.txt"), &link).unwrap();

        let refusal = world.reach().resolve("escape");

        assert!(refusal.is_err(), "{refusal:?}");
    }

    #[test]
    fn contains_is_false_for_a_sibling_whose_name_extends_a_root() {
        let world = World::new();
        let root = world.other.path().canonicalize().unwrap();
        let sibling = PathBuf::from(format!("{}-sibling/file", root.display()));

        assert!(!world.reach().contains(&sibling));
    }

    #[tokio::test]
    async fn read_in_reach_returns_a_roots_file() {
        let world = World::new();
        let theirs = world
            .other
            .path()
            .canonicalize()
            .unwrap()
            .join("theirs.txt");

        let outcome = execute_in_reach(
            &world.reach(),
            "Read",
            &serde_json::json!({ "path": theirs }).to_string(),
        )
        .await;

        assert!(!outcome.is_error, "{}", outcome.error_message);
        assert!(
            outcome.result_json.contains("theirs"),
            "{}",
            outcome.result_json
        );
    }
}
