//! The one call a resume makes into the roster restore, shared by every resume path.

use std::path::Path;

use tddy_service::proto::session::RestoredTab;

use super::{restore_tab_roster, RosterRestore};

impl crate::launch_ports::LaunchSessions {
    /// Restore the tabs of a resumed session's roster beside its main, each as its own launch chain
    /// says (`#session-liveness` 4/4). Every resume path restores the whole roster: nothing is
    /// skipped for where main runs.
    pub(crate) async fn restore_roster_on_resume(
        &self,
        os_user: &str,
        session_id: &str,
        session_dir: &Path,
        worktree: &Path,
    ) -> Vec<RestoredTab> {
        restore_tab_roster(RosterRestore {
            session_id,
            session_dir,
            worktree,
            os_user,
            cli_manager: &self.cli_manager,
            coding_agents: &self.coding_agents,
            roster_store: self.cli_manager.tab_rosters(),
        })
        .await
    }
}
