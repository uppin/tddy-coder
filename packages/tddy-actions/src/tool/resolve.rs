//! Resolving a tool against a host: which file its binary is, and what it needs readable.

use std::path::{Path, PathBuf};

use super::NetworkSection;

/// The environment a tool resolves against. Production uses the daemon's own process
/// ([`ProcessToolHost`]); tests inject a fixture so resolution is host-independent.
pub trait ToolHost: Send + Sync {
    /// The value of an environment variable, if set and non-empty.
    fn env_var(&self, name: &str) -> Option<String>;
    /// The directories of `PATH`, in search order.
    fn path_dirs(&self) -> Vec<PathBuf>;
    /// Whether `path` is an existing regular file the current user may execute.
    fn is_executable_file(&self, path: &Path) -> bool;
    /// `path` with every symlink resolved.
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
}

/// The daemon process as a [`ToolHost`]: its environment, its `PATH`, its filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessToolHost;

impl ToolHost for ProcessToolHost {
    fn env_var(&self, name: &str) -> Option<String> {
        // TODO(tool-defs): implement.
        let _ = name;
        todo!("tool-defs: ProcessToolHost::env_var")
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        // TODO(tool-defs): implement.
        todo!("tool-defs: ProcessToolHost::path_dirs")
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        // TODO(tool-defs): implement.
        let _ = path;
        todo!("tool-defs: ProcessToolHost::is_executable_file")
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        // TODO(tool-defs): implement.
        let _ = path;
        todo!("tool-defs: ProcessToolHost::canonicalize")
    }
}

/// A tool as this host satisfies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTool {
    pub id: String,
    /// The name the binary is reachable by on a jail's `PATH` (the definition's `binary.command`).
    pub command: String,
    /// The binary file, canonical — never a symlink, never a directory.
    pub binary: PathBuf,
    /// The definition's extra `reads`.
    pub reads: Vec<PathBuf>,
    /// The definition's `network:` section, carried so a tool set's egress can be lowered from its
    /// resolution alone (`#agent-tools 3/12`).
    pub network: NetworkSection,
}
