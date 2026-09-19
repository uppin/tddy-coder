//! What a daemon shuts down when it is asked to stop.
//!
//! One named seam rather than an inline closure in `tddy-daemon`'s signal handler, because a
//! shutdown that reaches only some of a process's children is indistinguishable, from the outside,
//! from one that reached all of them — which is how the workspace jails came to be missed entirely
//! (`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`).

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Stop every child process this daemon spawned that must not outlive it.
    ///
    /// The named seam `tddy-daemon`'s SIGTERM handler drives. It used to be an inline closure
    /// reaching `cli_sessions.kill_all()` and the index daemon only, so a workspace jail — a
    /// `tddy-sandbox-runner` this daemon started and is the only holder of — survived the daemon
    /// that spawned it
    /// (`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`).
    ///
    /// Scoped to what this host *holds*: the CLI sessions and the workspace jails. The index
    /// daemon belongs to the runtime, which stops it beside this call. The crash-detector and
    /// restart-policy half of that entry stays open.
    pub async fn shut_down_children(&self) {
        self.claude_cli_manager.kill_all().await;
        let stopped = self.workspace_sandboxes.stop_all().await;
        if !stopped.is_empty() {
            log::info!(
                "shutdown: stopped {} workspace jail(s): {stopped:?}",
                stopped.len()
            );
        }
    }
}
