//! Where the jail's binaries and read paths are found on the host. Moved out of
//! `sandbox_session.rs` unchanged (`#tab-hardening` 3/4, file length gate); its items are
//! re-exported there, so every path is as before.

use std::path::{Path, PathBuf};

/// Resolve the `tddy-tools` binary for sandbox MCP and hook wiring.
///
/// Priority: explicit config → `CARGO_BIN_EXE_tddy-tools` (cargo test) → sibling of
/// `current_exe()` (handles integration tests living in `target/debug/deps/`) → `"tddy-tools"`.
pub fn resolve_tddy_tools_path(configured: Option<&str>) -> String {
    if let Some(path) = configured.filter(|s| !s.trim().is_empty()) {
        return path.to_string();
    }
    if let Ok(bin) = std::env::var("CARGO_BIN_EXE_tddy-tools") {
        if !bin.trim().is_empty() {
            return bin;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mut bin_dir) = exe.parent().map(|p| p.to_path_buf()) {
            if bin_dir.file_name().and_then(|n| n.to_str()) == Some("deps") {
                bin_dir.pop();
            }
            let candidate = bin_dir.join("tddy-tools");
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
        if let Some(parent) = exe.parent() {
            let sibling = parent.join("tddy-tools");
            if sibling.is_file() {
                return sibling.to_string_lossy().into_owned();
            }
        }
    }
    "tddy-tools".to_string()
}

/// Resolve the `tddy-sandbox-runner` binary for in-jail sandbox sessions.
///
/// Priority: `CARGO_BIN_EXE_tddy-sandbox-runner` (cargo test) → sibling of
/// `current_exe()` → `"tddy-sandbox-runner"`.
pub fn resolve_sandbox_runner_path() -> String {
    if let Ok(bin) = std::env::var("CARGO_BIN_EXE_tddy-sandbox-runner") {
        if !bin.trim().is_empty() {
            return bin;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mut bin_dir) = exe.parent().map(|p| p.to_path_buf()) {
            if bin_dir.file_name().and_then(|n| n.to_str()) == Some("deps") {
                bin_dir.pop();
            }
            let candidate = bin_dir.join("tddy-sandbox-runner");
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
        if let Some(parent) = exe.parent() {
            let sibling = parent.join("tddy-sandbox-runner");
            if sibling.is_file() {
                return sibling.to_string_lossy().into_owned();
            }
        }
    }
    "tddy-sandbox-runner".to_string()
}

#[cfg(target_os = "macos")]
pub fn detect_allow_read_paths() -> Vec<PathBuf> {
    tddy_sandbox_darwin::detect_allow_read_paths()
}

#[cfg(not(target_os = "macos"))]
pub fn detect_allow_read_paths() -> Vec<PathBuf> {
    vec![]
}

fn push_parent_allow_path(paths: &mut Vec<PathBuf>, binary: &str) {
    let path = Path::new(binary);
    if !path.is_absolute() {
        return;
    }
    if let Some(parent) = path.parent() {
        if !paths.iter().any(|existing| existing == parent) {
            paths.push(parent.to_path_buf());
        }
    }
}

/// Paths from `otool -L` needed to load a Mach-O binary inside Seatbelt.
#[cfg(target_os = "macos")]
fn detect_binary_load_paths(binary: &str) -> Vec<PathBuf> {
    let Ok(output) = std::process::Command::new("otool")
        .args(["-L", binary])
        .output()
    else {
        return vec![];
    };
    if !output.status.success() {
        return vec![];
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut paths = Vec::new();
    for line in text.lines().skip(1) {
        let lib = line.split_whitespace().next().unwrap_or("");
        if lib.is_empty() || !lib.starts_with('/') {
            continue;
        }
        let lib_path = Path::new(lib);
        if let Some(parent) = lib_path.parent() {
            if !paths.iter().any(|existing| existing == parent) {
                paths.push(parent.to_path_buf());
            }
        }
    }
    paths
}

#[cfg(not(target_os = "macos"))]
fn detect_binary_load_paths(_binary: &str) -> Vec<PathBuf> {
    vec![]
}

fn canonical_binary_path(binary: &str) -> Option<PathBuf> {
    canonical_binary_path_on(&tddy_fs::DiskFs, binary)
}

/// [`canonical_binary_path`] through the filesystem port.
///
/// A bare name still resolves off the working directory: only an absolute path names a file the
/// port can resolve, and the relative branch keeps the `Path` spelling it had.
fn canonical_binary_path_on(fs: &dyn tddy_fs::Fs, binary: &str) -> Option<PathBuf> {
    let path = Path::new(binary);
    if path.is_absolute() {
        fs.canonicalize(path).ok()
    } else {
        std::env::current_dir().ok()?.join(path).canonicalize().ok()
    }
}

/// Toolchain paths plus the parent dir of the sandbox-runner binary (the argv's first entry). What
/// the process the runner execs needs to read comes from its tool definition, not from its argv.
pub fn build_allow_read_paths(runner_argv: &[String]) -> Vec<PathBuf> {
    let mut paths = detect_allow_read_paths();
    if let Some(tool) = runner_argv.first() {
        if let Some(path) = canonical_binary_path(tool) {
            push_parent_allow_path(&mut paths, &path.to_string_lossy());
            paths.extend(detect_binary_load_paths(&path.to_string_lossy()));
        }
    }
    paths
}
