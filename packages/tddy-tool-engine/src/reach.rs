//! An orchestrator's read reach (`#orchestra` 1/9): the roots its `Read`, `Grep` and `Glob` may
//! answer from. The daemon builds a [`ReadReach`] per call from every tddy worktree on the host, the
//! coding agents' declared home directories and the session's grants; this module only applies it.
//!
//! A relative path keeps meaning the caller's own home worktree. An absolute path is answered when
//! it lies inside a root, after symlinks are resolved; anything else is refused with a message that
//! names `RequestPathAccess`, the tool that asks the operator for more.

use std::path::{Component, Path, PathBuf};

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
    /// reach) and whose absolute paths may lie under `home` or any of `roots`. Roots are compared
    /// with symlinks resolved, so each is canonicalised here when it exists.
    pub fn new(home: PathBuf, roots: Vec<PathBuf>) -> Self {
        let roots = roots
            .into_iter()
            .map(|root| root.canonicalize().unwrap_or(root))
            .collect();
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

    /// Whether `path` (absolute, symlinks resolved) lies under the home or a root. Containment is
    /// by path component, so `/srv/repo-sibling` is not inside `/srv/repo`.
    pub fn contains(&self, path: &Path) -> bool {
        path.starts_with(&self.home) || self.roots.iter().any(|root| path.starts_with(root))
    }

    /// Resolve a tool's path argument: relative under the home, absolute as given, then canonical
    /// and inside reach — or a refusal naming [`REQUEST_PATH_ACCESS_TOOL`].
    pub fn resolve(&self, arg_path: &str) -> Result<PathBuf, String> {
        let candidate = if Path::new(arg_path).is_absolute() {
            PathBuf::from(arg_path)
        } else {
            self.home.join(arg_path)
        };
        if candidate
            .components()
            .any(|component| component == Component::ParentDir)
        {
            return Err(format!(
                "path contains '..' component: {}",
                candidate.display()
            ));
        }
        // The nearest existing ancestor decides: a path that does not exist yet is judged by the
        // directory it would be created in, and a symlink anywhere on the way is followed.
        let mut existing = candidate.clone();
        while !existing.exists() {
            if !existing.pop() {
                return Err(self.refusal(&candidate));
            }
        }
        let canonical_ancestor = existing
            .canonicalize()
            .map_err(|e| format!("cannot canonicalize path: {e}"))?;
        if !self.contains(&canonical_ancestor) {
            return Err(self.refusal(&candidate));
        }
        if existing == candidate {
            return Ok(canonical_ancestor);
        }
        let missing = candidate
            .strip_prefix(&existing)
            .map_err(|e| format!("cannot resolve path: {e}"))?;
        Ok(canonical_ancestor.join(missing))
    }

    fn refusal(&self, path: &Path) -> String {
        let roots: Vec<String> = std::iter::once(&self.home)
            .chain(&self.roots)
            .map(|root| root.display().to_string())
            .collect();
        format!(
            "{} is outside the paths this session may read ({}); ask the operator for it with {}",
            path.display(),
            roots.join(", "),
            REQUEST_PATH_ACCESS_TOOL
        )
    }
}

/// Run `Read`, `Grep` or `Glob` within `reach`. Any other tool is an error outcome.
pub async fn execute_in_reach(reach: &ReadReach, tool_name: &str, args_json: &str) -> ToolOutcome {
    let args: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::err(format!("invalid args_json: {e}")),
    };
    match tool_name {
        "Read" => read_in_reach(reach, &args),
        "Grep" => grep_in_reach(reach, &args).await,
        "Glob" => glob_in_reach(reach, &args),
        other => ToolOutcome::err(format!("{other} is not answered within a read reach")),
    }
}

fn read_in_reach(reach: &ReadReach, args: &serde_json::Value) -> ToolOutcome {
    let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
        return ToolOutcome::err("Read: missing 'path' argument");
    };
    match reach.resolve(path) {
        Ok(resolved) => crate::read_resolved(&resolved, args),
        Err(e) => ToolOutcome::err(format!("Read: {e}")),
    }
}

async fn grep_in_reach(reach: &ReadReach, args: &serde_json::Value) -> ToolOutcome {
    let target = match args.get("path").and_then(|v| v.as_str()) {
        None => ".".to_string(),
        Some(path) => match reach.resolve(path) {
            Ok(resolved) => resolved.display().to_string(),
            Err(e) => return ToolOutcome::err(format!("Grep: {e}")),
        },
    };
    crate::search_tools::grep_target(reach.home(), &target, args).await
}

fn glob_in_reach(reach: &ReadReach, args: &serde_json::Value) -> ToolOutcome {
    let Some(pattern) = args.get("pattern").and_then(|v| v.as_str()) else {
        return ToolOutcome::err("Glob: missing 'pattern' argument");
    };
    if let Err(e) = reach.resolve(&literal_prefix(pattern)) {
        return ToolOutcome::err(format!("Glob: {e}"));
    }
    // An entry that is a symlink out of reach is dropped, not listed.
    crate::search_tools::glob_where(reach.home(), args, |entry| {
        entry
            .canonicalize()
            .map(|canonical| reach.contains(&canonical))
            .unwrap_or(false)
    })
}

/// The leading components of a glob `pattern` that hold no wildcard: the directory the match
/// starts from. A pattern that starts with a wildcard has none, which is the home itself.
fn literal_prefix(pattern: &str) -> String {
    let mut prefix = PathBuf::new();
    for component in Path::new(pattern).components() {
        if component
            .as_os_str()
            .to_string_lossy()
            .contains(['*', '?', '[', '{'])
        {
            break;
        }
        prefix.push(component);
    }
    if prefix.as_os_str().is_empty() {
        ".".to_string()
    } else {
        prefix.display().to_string()
    }
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
