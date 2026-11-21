//! Where a jailed shell tab keeps its files (`#session-liveness` 5/8).
//!
//! A session's **main** jail keeps its files per session: its runner's tool-IPC socket hashes the
//! session id, and its ready marker, exec plan and profile live in `<session>/sandbox/`. A second
//! runner of the same session — a tab's jail — writing those paths would collide with the main's
//! (and a main launch deletes the socket and marker before it spawns). So each tab jail gets paths
//! of its own, under `<session>/sandbox/tabs/<terminal_id>/`, and a socket hashing the session *and*
//! the terminal id.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-sandbox-shell-tab.md

use std::path::{Path, PathBuf};

use tddy_rpc::Status;
use tddy_sandbox::SandboxSpec;

/// The directory under a session's `sandbox/` holding one directory per jailed tab.
pub const TAB_JAILS_DIR: &str = "tabs";

/// The files one tab's jail is started with, every one distinct from its session's main jail's and
/// from every other tab's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabJailPaths {
    /// `<session>/sandbox/tabs/<terminal_id>`: the tab jail's project root, holding the rest.
    pub root: PathBuf,
    /// The AF_UNIX socket the jail's runner serves the in-jail tools on: a short out-of-tree path
    /// (it must fit `SUN_LEN`) hashing the session id and the terminal id.
    pub ipc_socket: PathBuf,
    /// The marker the runner writes once it is ready: `<root>/sandbox.ready`.
    pub ready_marker: PathBuf,
    /// The plan the runner execs: `<root>/exec-plan.json`.
    pub exec_plan: PathBuf,
    /// The jail's profile: `<root>/sandbox.sb`.
    pub profile: PathBuf,
    /// Where the jail's egress logs are written: `<root>/egress`.
    pub egress_dir: PathBuf,
    /// The jail's scratch home and tmp: `<root>/.work`.
    pub scratch_dir: PathBuf,
}

impl TabJailPaths {
    /// The paths of the jail of tab `terminal_id` of session `session_id`, whose directory is
    /// `session_dir`. A terminal id that is not one path segment is `INVALID_ARGUMENT`: a roster
    /// entry read back from disk must not place a jail's files outside its session.
    pub fn for_tab(
        session_dir: &Path,
        session_id: &str,
        terminal_id: &str,
    ) -> Result<TabJailPaths, Status> {
        if !is_one_path_segment(terminal_id) {
            return Err(Status::invalid_argument(format!(
                "terminal id {terminal_id:?} is not one path segment; a tab jail's files stay inside its session"
            )));
        }
        let root = session_dir
            .join("sandbox")
            .join(TAB_JAILS_DIR)
            .join(terminal_id);
        Ok(TabJailPaths {
            ipc_socket: tab_ipc_socket_path(session_id, terminal_id),
            ready_marker: root.join("sandbox.ready"),
            exec_plan: root.join("exec-plan.json"),
            profile: root.join("sandbox.sb"),
            egress_dir: root.join("egress"),
            scratch_dir: root.join(".work"),
            root,
        })
    }
}

/// Whether `id` is exactly one ordinary path segment: it names a directory directly under another.
fn is_one_path_segment(id: &str) -> bool {
    let mut components = Path::new(id).components();
    matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    ) && !id.contains(['/', '\\', '\0'])
}

/// The tool-IPC socket of the jail of tab `terminal_id` of session `session_id`: short enough for
/// `SUN_LEN`, and distinct from the session's main socket
/// ([`tddy_sandbox::SandboxSpec::short_ipc_socket_path`]) and from every other tab's.
pub fn tab_ipc_socket_path(session_id: &str, terminal_id: &str) -> PathBuf {
    // The main jail's socket hashes the session id; hashing the pair keeps a tab's socket in the
    // same short temp directory yet distinct from the main's and from every other tab's.
    SandboxSpec::short_ipc_socket_path(&format!("{session_id}\0{terminal_id}"))
}

#[cfg(test)]
#[path = "tab_jail_paths_tests.rs"]
mod tests;
