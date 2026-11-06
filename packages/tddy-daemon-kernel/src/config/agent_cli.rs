//! Locating the binaries of the host's CLI agents. Nothing here reads `daemon.yaml`: an agent's
//! binary, its hook url and its home all come from its tool file (`#agent-tools 19/19`).

use std::path::{Path, PathBuf};

/// A wrapper-shim `bin` directory whose `claude` merely re-execs the real binary from `$PATH`
/// (e.g. Superset's `~/.superset/bin`). Such a shim can't resolve inside the jail — its `PATH` is
/// only `/usr/bin:/bin` — so host resolution must skip these dirs. See the `claude-sandbox` script.
#[must_use]
pub fn is_wrapper_shim_dir(dir: &Path) -> bool {
    let ends_with_bin = dir.file_name().is_some_and(|n| n == "bin");
    ends_with_bin
        && dir.components().any(|c| match c {
            std::path::Component::Normal(n) => n.to_string_lossy().starts_with(".superset"),
            _ => false,
        })
}

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(p: &Path) -> bool {
    p.is_file()
}

/// A configured harness binary taken as named when it is already a path (contains `/`); a bare
/// name resolved on the host through `find`, falling back to the name itself when nothing is found
/// (the spawner's error then names it). Pure apart from `find`, so the precedence is unit-testable.
pub fn resolve_harness_binary_name(
    configured: String,
    find: impl Fn(&str) -> Option<PathBuf>,
) -> String {
    if configured.contains('/') {
        return configured;
    }
    find(&configured)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or(configured)
}

/// Locate a harness binary as an absolute path, injectable: prefer `<home>/.local/bin/<name>` (the
/// common user-local install), then scan `path_dirs` skipping wrapper-shim dirs. `None` if none is
/// found. The home and search dirs are parameters (not read from env) so the resolution logic is
/// unit-testable. `find_real_claude_on_host` and the cursor resolver delegate to this with their
/// own binary names.
pub(crate) fn find_real_harness_in(
    name: &str,
    home: Option<&Path>,
    path_dirs: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(home) = home {
        let candidate = home.join(".local/bin").join(name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    path_dirs
        .iter()
        .filter(|d| !d.as_os_str().is_empty() && !is_wrapper_shim_dir(d))
        .map(|d| d.join(name))
        .find(|c| is_executable_file(c))
}

/// Locate a registered CLI harness's binary (e.g. `muse`, `opencode`) on the host as an absolute
/// path: prefer `~/.local/bin/<name>`, then scan `$PATH` skipping wrapper-shim dirs. `None` if none
/// is found. This is the resolution a bare harness name needs on a systemd daemon, whose own `PATH`
/// is minimal and never holds the user-local install dirs.
pub fn find_real_harness_on_host(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    find_real_harness_in(name, home.as_deref(), &path_dirs)
}
