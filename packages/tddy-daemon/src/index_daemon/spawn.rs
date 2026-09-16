//! What to start, and what a started one is: the spawn plan an `index_daemon:` configuration
//! section asks for, the record of a bound process, and where the program is found.

use std::path::PathBuf;
use std::time::Duration;

use tddy_task::TaskId;

/// What to run, where to reach it, and how patient to be with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDaemonSpawn {
    /// The `tddy-index-daemon` program to execute.
    pub program: PathBuf,
    /// The AF_UNIX socket the child is told to bind, and the one this daemon dials.
    pub socket_path: PathBuf,
    /// How long the child has to bind that socket before the start is refused.
    pub ready_timeout: Duration,
    /// How long the process may sit unused before [`crate::index_daemon::IndexDaemonRegistry::reap_idle`]
    /// stops it.
    pub idle_timeout: Duration,
}

impl IndexDaemonSpawn {
    /// The spawn plan an `index_daemon:` configuration section asks for.
    pub fn from_config(config: &tddy_daemon_kernel::config::IndexDaemonConfig) -> Self {
        Self {
            program: config
                .binary_path
                .clone()
                .unwrap_or_else(resolve_index_daemon_path),
            socket_path: config.resolved_socket_path(),
            ready_timeout: Duration::from_secs(config.ready_timeout_secs),
            idle_timeout: Duration::from_secs(config.idle_timeout_secs),
        }
    }
}

/// A started, bound index daemon: the task that owns its process, and where to reach it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDaemon {
    pub task_id: TaskId,
    pub socket_path: PathBuf,
}

/// Resolve the `tddy-index-daemon` binary.
///
/// The three tiers of `tddy_daemon_sandbox::resolve_sandbox_runner_path`:
/// `CARGO_BIN_EXE_tddy-index-daemon` (a cargo test in the crate that owns the binary) → a sibling
/// of `current_exe()`, with the `deps/` hop an integration-test binary needs → the bare name, left
/// to `PATH`, which is where `./install` puts it.
pub fn resolve_index_daemon_path() -> PathBuf {
    const PROGRAM: &str = "tddy-index-daemon";

    if let Ok(bin) = std::env::var("CARGO_BIN_EXE_tddy-index-daemon") {
        if !bin.trim().is_empty() {
            return PathBuf::from(bin);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mut bin_dir) = exe.parent().map(|dir| dir.to_path_buf()) {
            if bin_dir.file_name().and_then(|name| name.to_str()) == Some("deps") {
                bin_dir.pop();
            }
            let candidate = bin_dir.join(PROGRAM);
            if candidate.is_file() {
                return candidate;
            }
        }
        if let Some(sibling) = exe.parent().map(|dir| dir.join(PROGRAM)) {
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from(PROGRAM)
}
