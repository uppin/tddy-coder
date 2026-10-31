//! Resolving a tool against a host: which file its binary is, and what it needs readable.

use std::path::{Path, PathBuf};

use super::{NetworkSection, TargetBinary, ToolBinary, ToolError};

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
    /// Where `binary`'s producing target put its output on this host, once that target has built
    /// (`#agent-tools 4/12`). Only a host that knows a build graph — `tddy-build`'s executor —
    /// answers; every other host has no targets, and a tool naming one is unresolvable there.
    fn target_output(&self, binary: &TargetBinary) -> Option<PathBuf> {
        let _ = binary;
        None
    }
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

/// Where `tool`'s binary is when it declares `sibling_of` (`#agent-tools 11/12`): `binary.command`
/// in the directory of `dep`'s resolved binary, canonical. Refused as
/// [`ToolError::Unresolvable`] when no executable file is there, naming both the path searched and
/// the dep binary it was searched beside — never a fall back to the `PATH`.
///
/// [`super::ToolRegistry::resolve_closure`] calls it once `dep` is resolved; `binary.env` is tried
/// before it.
pub fn resolve_sibling(
    tool: &str,
    binary: &ToolBinary,
    dep: &ResolvedTool,
    host: &dyn ToolHost,
) -> Result<PathBuf, ToolError> {
    // TODO(acp-agents): implement.
    let _ = (tool, binary, dep, host);
    todo!("acp-agents: resolve_sibling")
}
