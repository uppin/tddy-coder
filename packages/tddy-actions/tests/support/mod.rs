//! Test support for declared tools: tool files on disk and a fixture [`ToolHost`].

#![allow(dead_code)] // each suite uses its own subset

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tddy_actions::tool::ToolHost;

/// A `<tddyhome>/tools` directory a test writes tool files into.
pub struct AToolsDir {
    dir: tempfile::TempDir,
}

pub fn a_tools_dir() -> AToolsDir {
    AToolsDir {
        dir: tempfile::tempdir().expect("a tools dir"),
    }
}

impl AToolsDir {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Write `<file>` with `yaml` and return its path.
    pub fn with_file(self, file: &str, yaml: &str) -> Self {
        std::fs::write(self.dir.path().join(file), yaml).expect("a tool file");
        self
    }

    pub fn file(&self, file: &str) -> PathBuf {
        self.dir.path().join(file)
    }
}

/// A minimal valid tool file for `id` whose binary is `command`, depending on `deps`.
pub fn a_tool_yaml(id: &str, command: &str, deps: &[&str]) -> String {
    let deps = deps
        .iter()
        .map(|d| format!("\"{d}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "schema_version: 1\nid: {id}\nsummary: the {id} tool\nbinary:\n  command: {command}\ndeps: [{deps}]\n"
    )
}

/// A host with a declared environment, `PATH` and filesystem — nothing of the real machine.
#[derive(Default, Clone)]
pub struct AFixtureHost {
    env: BTreeMap<String, String>,
    path: Vec<PathBuf>,
    executables: BTreeSet<PathBuf>,
    links: BTreeMap<PathBuf, PathBuf>,
}

pub fn a_host() -> AFixtureHost {
    AFixtureHost::default()
}

impl AFixtureHost {
    pub fn with_env(mut self, name: &str, value: &str) -> Self {
        self.env.insert(name.into(), value.into());
        self
    }

    pub fn with_path(mut self, dirs: &[&str]) -> Self {
        self.path = dirs.iter().map(PathBuf::from).collect();
        self
    }

    /// An executable regular file at `path`.
    pub fn with_executable(mut self, path: &str) -> Self {
        self.executables.insert(PathBuf::from(path));
        self
    }

    /// `link` is a symlink to the executable `target` (as Homebrew's `bin/` links into `Cellar`).
    pub fn with_link(mut self, link: &str, target: &str) -> Self {
        self.links
            .insert(PathBuf::from(link), PathBuf::from(target));
        self.executables.insert(PathBuf::from(target));
        self
    }
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        self.path.clone()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        self.executables.contains(path) || self.links.contains_key(path)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        if let Some(target) = self.links.get(path) {
            return Ok(target.clone());
        }
        if self.executables.contains(path) {
            return Ok(path.to_path_buf());
        }
        Err(std::io::Error::from(std::io::ErrorKind::NotFound))
    }
}
