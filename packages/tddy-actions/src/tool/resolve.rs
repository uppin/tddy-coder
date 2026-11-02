//! Resolving a tool against a host: which file its binary is, and what it needs readable.

use std::path::{Path, PathBuf};

use super::{ToolDef, ToolError};

/// The environment a tool resolves against. Production uses the daemon's own process
/// ([`ProcessToolHost`]); tests inject a fixture so resolution is host-independent.
pub trait ToolHost: Send + Sync {
    /// The value of an environment variable, if set and non-empty.
    fn env_var(&self, name: &str) -> Option<String>;
    /// The directories of `PATH`, in search order.
    fn path_dirs(&self) -> Vec<PathBuf>;
    /// Whether `path` is an existing regular file with an execute bit set.
    fn is_executable_file(&self, path: &Path) -> bool;
    /// `path` with every symlink resolved.
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
}

/// The daemon process as a [`ToolHost`]: its environment, its `PATH`, its filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessToolHost;

impl ToolHost for ProcessToolHost {
    fn env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }

    /// Relative and empty `PATH` entries are dropped. They name the daemon's working directory, and
    /// no jail should be granted a binary from wherever the daemon happened to start.
    fn path_dirs(&self) -> Vec<PathBuf> {
        std::env::var_os("PATH")
            .map(|path| {
                std::env::split_paths(&path)
                    .filter(|dir| dir.is_absolute())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        is_executable(path)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
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
}

/// Resolve one definition against `host`: the canonical binary it names, and its declared reads.
pub(super) fn resolve_tool(def: &ToolDef, host: &dyn ToolHost) -> Result<ResolvedTool, ToolError> {
    Ok(ResolvedTool {
        id: def.id.clone(),
        command: def.binary.command.clone(),
        binary: locate_binary(def, host)?,
        reads: def.reads.clone(),
    })
}

/// The `env` override first, then the first `PATH` directory holding the command. An override that
/// does not name an executable refuses the tool: it never falls back to the search.
fn locate_binary(def: &ToolDef, host: &dyn ToolHost) -> Result<PathBuf, ToolError> {
    let mut searched = Vec::new();
    if let Some(var) = &def.binary.env {
        match host.env_var(var) {
            Some(value) => return override_binary(def, var, &value, host),
            None => searched.push(format!("{var} (unset)")),
        }
    }
    let dirs = host.path_dirs();
    for dir in &dirs {
        let candidate = dir.join(&def.binary.command);
        if host.is_executable_file(&candidate) {
            return canonical(def, &candidate, host);
        }
    }
    searched.push(format!("PATH ({})", join_dirs(&dirs)));
    Err(unresolvable(def, searched.join(", ")))
}

fn override_binary(
    def: &ToolDef,
    var: &str,
    value: &str,
    host: &dyn ToolHost,
) -> Result<PathBuf, ToolError> {
    let candidate = PathBuf::from(value);
    if host.is_executable_file(&candidate) {
        return canonical(def, &candidate, host);
    }
    Err(unresolvable(
        def,
        format!("{var} ({value}: not an executable file)"),
    ))
}

fn canonical(def: &ToolDef, path: &Path, host: &dyn ToolHost) -> Result<PathBuf, ToolError> {
    host.canonicalize(path)
        .map_err(|e| unresolvable(def, format!("{} (cannot be resolved: {e})", path.display())))
}

fn unresolvable(def: &ToolDef, searched: String) -> ToolError {
    ToolError::Unresolvable {
        tool: def.id.clone(),
        searched,
    }
}

fn join_dirs(dirs: &[PathBuf]) -> String {
    dirs.iter()
        .map(|dir| dir.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
}
