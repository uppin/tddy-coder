//! Generic read-grant detectors for confined process execution.
//!
//! Product-specific recipes (e.g. Claude CLI) compose these helpers in `tddy-sandbox-recipes`.

use std::path::{Path, PathBuf};

use crate::builder::{ReadReason, ReadSpec};

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
    if let Some(parent) = binary.parent() {
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
