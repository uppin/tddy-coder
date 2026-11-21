//! What the `TabJails` suites share: the launch of a tab's jail. Its jails are
//! [`crate::jail_test_support`]'s stand-ins.

use std::path::PathBuf;

use super::{TabJailLaunch, TabJailPaths};

/// The launch of tab `terminal_id`'s jail, with paths that only have to be distinct.
pub(super) fn a_launch(session_id: &str, terminal_id: &str) -> TabJailLaunch {
    let root = PathBuf::from("/sessions")
        .join(session_id)
        .join("sandbox/tabs")
        .join(terminal_id);
    TabJailLaunch {
        session_id: session_id.to_string(),
        terminal_id: terminal_id.to_string(),
        session_dir: PathBuf::from("/sessions").join(session_id),
        worktree: PathBuf::from("/worktrees").join(session_id),
        paths: TabJailPaths {
            root: root.clone(),
            ipc_socket: root.join("tab.sock"),
            ready_marker: root.join("sandbox.ready"),
            exec_plan: root.join("exec-plan.json"),
            profile: root.join("sandbox.sb"),
            egress_dir: root.join("egress"),
            scratch_dir: root.join(".work"),
        },
        os_user: "tddy".to_string(),
        login_shell: "/bin/zsh".to_string(),
        subject: "ada".to_string(),
    }
}
