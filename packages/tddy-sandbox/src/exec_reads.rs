//! Generic read-grant detectors for confined process execution.
//!
//! Product-specific recipes (e.g. Claude CLI) compose these helpers in `tddy-sandbox-recipes`.

use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

use crate::builder::{ReadReason, ReadSpec};

/// Read-only grants for ancestor directories needed to traverse to `path` under Seatbelt.
///
/// A `(subpath "/Users/foo/bar")` grant does not permit `lstat("/Users")` — each directory on the
/// path from `/` must be readable for `realpath` and Node module resolution.
pub fn path_traversal_reads(path: &Path) -> Vec<ReadSpec> {
    let resolved = if path.is_absolute() {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };

    let mut reads = Vec::new();
    let mut current = resolved.parent();
    while let Some(dir) = current {
        if dir.as_os_str().is_empty() || dir == Path::new("/") {
            break;
        }
        reads.push(ReadSpec::subpath(dir, ReadReason::BinaryDeps));
        current = dir.parent();
    }
    reads
}

/// Read grants for executing a host binary: OS baseline, optional toolchain dirs, and `otool -L` deps.
pub fn process_exec_reads(binary: &Path) -> Vec<ReadSpec> {
    let mut reads = system_baseline_reads();
    reads.extend(detect_toolchain_reads());
    reads.extend(binary_exec_reads(binary));
    reads
}

/// The always-needed OS read set for a CLI under the jail.
#[cfg(target_os = "macos")]
pub fn system_baseline_reads() -> Vec<ReadSpec> {
    let exec_subpath = |p: &str| ReadSpec::subpath(p, ReadReason::SystemLibs).executable();
    let subpath = |p: &str| ReadSpec::subpath(p, ReadReason::SystemLibs);
    let cache = |p: &str| ReadSpec::subpath(p, ReadReason::OsCaches);
    vec![
        ReadSpec::literal("/", ReadReason::DyldRoot),
        subpath("/usr/lib"),
        exec_subpath("/usr/libexec"),
        subpath("/System"),
        subpath("/Library"),
        subpath("/private/var/db/dyld"),
        subpath("/private/etc"),
        exec_subpath("/usr/bin"),
        exec_subpath("/bin"),
        exec_subpath("/sbin"),
        cache("/private/var/folders"),
        cache("/usr/share/zoneinfo"),
        cache("/private/var/db/timezone"),
        subpath("/private/var/select"),
        subpath("/usr/share/icu"),
        ReadSpec::literal("/dev/null", ReadReason::SystemLibs),
        ReadSpec::literal("/dev/zero", ReadReason::SystemLibs),
        ReadSpec::literal("/dev/random", ReadReason::SystemLibs),
        ReadSpec::literal("/dev/urandom", ReadReason::SystemLibs),
        ReadSpec::literal("/dev/dtracehelper", ReadReason::SystemLibs),
        ReadSpec::literal("/dev/stdin", ReadReason::SystemLibs),
        ReadSpec::literal("/dev/stdout", ReadReason::SystemLibs),
        ReadSpec::literal("/dev/stderr", ReadReason::SystemLibs),
        ReadSpec::regex("^/dev/fd/[0-9]+$", ReadReason::Pty),
        ReadSpec::literal("/dev/ptmx", ReadReason::Pty),
        ReadSpec::regex("^/dev/tty.*", ReadReason::Pty),
        ReadSpec::regex("^/dev/ttys[0-9]+$", ReadReason::Pty),
    ]
}

#[cfg(not(target_os = "macos"))]
pub fn system_baseline_reads() -> Vec<ReadSpec> {
    let exec_subpath = |p: &str| ReadSpec::subpath(p, ReadReason::SystemLibs).executable();
    let subpath = |p: &str| ReadSpec::subpath(p, ReadReason::SystemLibs);
    [
        exec_subpath("/usr/bin"),
        exec_subpath("/bin"),
        subpath("/usr/lib"),
        subpath("/lib"),
        subpath("/lib64"),
        subpath("/usr/lib64"),
        subpath("/etc/ssl/certs"),
        ReadSpec::literal("/etc/resolv.conf", ReadReason::SystemLibs),
        ReadSpec::literal("/etc/ld.so.cache", ReadReason::SystemLibs),
        subpath("/usr/share/zoneinfo"),
    ]
    .into_iter()
    .filter(|r| std::path::Path::new(&r.host).exists())
    .collect()
}

#[cfg(target_os = "macos")]
pub fn binary_exec_reads(binary: &Path) -> Vec<ReadSpec> {
    let mut reads = Vec::new();
    // `Path::new("claude").parent()` is `Some("")`, not `None`, for a bare binary name (no
    // directory component). An empty subpath must never become a grant: macOS `sandbox-exec`
    // rejects `(subpath "")`, and in the builder an empty enclosing subpath shadows every other
    // read in the allow-list. Skip it — callers should pass an absolute path.
    if let Some(parent) = binary.parent().filter(|p| !p.as_os_str().is_empty()) {
        reads.push(ReadSpec::subpath(parent, ReadReason::BinaryDeps).executable());
    }
    if let Ok(output) = std::process::Command::new("otool")
        .args(["-L"])
        .arg(binary)
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines().skip(1) {
                let lib = line.split_whitespace().next().unwrap_or("");
                if lib.is_empty() || !lib.starts_with('/') {
                    continue;
                }
                if let Some(parent) = Path::new(lib).parent() {
                    reads.push(ReadSpec::subpath(parent, ReadReason::BinaryDeps));
                }
            }
        }
    }
    reads
}

#[cfg(not(target_os = "macos"))]
pub fn binary_exec_reads(binary: &Path) -> Vec<ReadSpec> {
    binary
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|parent| vec![ReadSpec::subpath(parent, ReadReason::BinaryDeps).executable()])
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
pub fn detect_toolchain_reads() -> Vec<ReadSpec> {
    let mut reads = Vec::new();
    let mut push_dir = |dir: PathBuf| {
        if !reads.iter().any(|r: &ReadSpec| r.host == dir) {
            reads.push(ReadSpec::subpath(dir, ReadReason::Toolchain).executable());
        }
    };
    let run = |program: &str, args: &[&str]| -> Option<String> {
        let out = std::process::Command::new(program)
            .args(args)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    };
    if let Some(dev) = run("xcode-select", &["-p"]) {
        push_dir(PathBuf::from(dev));
    }
    if let Some(node) = run("which", &["node"]) {
        if let Some(parent) = Path::new(&node).parent() {
            push_dir(parent.to_path_buf());
        }
    }
    if let Some(brew) = run("brew", &["--prefix"]) {
        push_dir(PathBuf::from(brew));
    }
    reads
}

#[cfg(not(target_os = "macos"))]
pub fn detect_toolchain_reads() -> Vec<ReadSpec> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    // `PathBuf` is only imported at module scope on macOS (above), but `path_traversal_reads` and
    // its test are platform-agnostic — import it unconditionally here so the test builds on Linux.
    use std::path::{Path, PathBuf};

    /// A bare binary name has an *empty* parent path — `Path::parent` returns `Some("")`, not
    /// `None`. That empty subpath must never become a read grant: macOS `sandbox-exec` rejects
    /// `(subpath "")`, and in the builder it would shadow every other read in the allow-list.
    #[test]
    fn binary_exec_reads_skips_the_empty_parent_of_a_bare_binary_name() {
        let reads = binary_exec_reads(std::path::Path::new("claude"));
        assert!(
            reads.iter().all(|r| !r.host.as_os_str().is_empty()),
            "a bare binary name must not yield an empty-host read: {reads:?}"
        );
    }

    #[test]
    fn path_traversal_reads_include_ancestors_up_to_root() {
        let path = PathBuf::from("/Users/alice/.local/share/cursor-agent/versions/1.0/node");
        let reads = path_traversal_reads(&path);
        assert!(
            reads.iter().any(|r| r.host == Path::new("/Users")),
            "/Users must be readable for traversal: {reads:?}"
        );
        assert!(
            reads.iter().any(|r| r.host == Path::new("/Users/alice")),
            "user home ancestor must be readable: {reads:?}"
        );
    }
}
