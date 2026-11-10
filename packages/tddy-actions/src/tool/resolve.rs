//! Resolving a tool against a host: which file its binary is, and what it needs readable.

use std::path::{Path, PathBuf};

use super::{NetworkSection, TargetBinary, ToolBinary, ToolDef, ToolError};

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
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }

    /// Relative and empty `PATH` entries are dropped. They name the daemon's working directory, and
    /// no jail should be granted a binary from wherever the daemon happened to start.
    /// An unset `PATH` is an empty search, which refuses the tool — it never falls back to a grant.
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
    /// The definition's `network:` section, carried so a tool set's egress can be lowered from its
    /// resolution alone (`#agent-tools 3/12`).
    pub network: NetworkSection,
}

/// Resolve one definition against `host`: the canonical binary it names, and its declared reads.
/// `resolved` is the closure resolved so far, deps first, so a `sibling_of` dep is in it.
pub(super) fn resolve_tool(
    def: &ToolDef,
    host: &dyn ToolHost,
    resolved: &[ResolvedTool],
) -> Result<ResolvedTool, ToolError> {
    Ok(ResolvedTool {
        id: def.id.clone(),
        command: def.binary.command.clone(),
        binary: locate_binary(def, host, resolved)?,
        reads: def.reads.clone(),
        network: def.network.clone(),
    })
}

/// The producing target's output when the binary is one, then the `env` override, then the dep the
/// binary sits beside (`sibling_of`, which never searches the `PATH`), then the first `PATH`
/// directory holding the command. A target output that is not an executable, an override that does
/// not name one, and a sibling that is not beside its dep each refuse the tool: none falls back to
/// the search.
fn locate_binary(
    def: &ToolDef,
    host: &dyn ToolHost,
    resolved: &[ResolvedTool],
) -> Result<PathBuf, ToolError> {
    if let Some(target) = &def.binary.target {
        return match host.target_output(target) {
            Some(output) => target_output_binary(def, target, &output, host),
            None => Err(unresolvable(
                def,
                format!("{} output {}", target.target, target.path),
            )),
        };
    }
    let mut searched = Vec::new();
    if let Some(var) = &def.binary.env {
        match host.env_var(var) {
            Some(value) => return override_binary(def, var, &value, host),
            None => searched.push(format!("{var} (unset)")),
        }
    }
    if let Some(dep_id) = &def.binary.sibling_of {
        let dep = resolved
            .iter()
            .find(|tool| &tool.id == dep_id)
            .ok_or_else(|| {
                unresolvable(def, format!("{dep_id} (not resolved before {})", def.id))
            })?;
        return resolve_sibling(&def.id, &def.binary, dep, host);
    }
    let dirs = host.path_dirs();
    for dir in &dirs {
        let candidate = dir.join(&def.binary.command);
        if host.is_executable_file(&candidate) {
            return canonical(def, &candidate, host);
        }
    }
    if dirs.is_empty() {
        searched.push("PATH (empty)".to_string());
    } else {
        searched.push(format!("PATH ({})", join_dirs(&dirs)));
    }
    Err(unresolvable(def, searched.join(", ")))
}

fn target_output_binary(
    def: &ToolDef,
    target: &TargetBinary,
    output: &Path,
    host: &dyn ToolHost,
) -> Result<PathBuf, ToolError> {
    if host.is_executable_file(output) {
        return canonical(def, output, host);
    }
    Err(unresolvable(
        def,
        format!(
            "{} output {} (not an executable file)",
            target.target,
            output.display()
        ),
    ))
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
    let refused = |searched: String| ToolError::Unresolvable {
        tool: tool.to_string(),
        searched,
    };
    let candidate = dep.binary.with_file_name(&binary.command);
    let beside = format!(
        "{} (beside {} at {})",
        candidate.display(),
        dep.id,
        dep.binary.display()
    );
    if !host.is_executable_file(&candidate) {
        return Err(refused(beside));
    }
    host.canonicalize(&candidate)
        .map_err(|e| refused(format!("{} (cannot be resolved: {e})", candidate.display())))
}
