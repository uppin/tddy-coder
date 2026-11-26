//! The one call a resume makes into the roster restore, shared by every resume path.

use std::path::Path;

use tddy_service::proto::session::{RestoredTab, ResumeSessionRequest};

use super::{restore_tab_roster, RosterRestore};

impl crate::launch_ports::LaunchSessions {
    /// Restore the tabs of a resumed session's roster beside its main, each as its own launch chain
    /// says (`#session-liveness` 4/4). Every resume path restores the whole roster: nothing is
    /// skipped for where main runs. `subject` is who the resume's caller is, as their session
    /// token resolved: the owner a target tab's target is reached as (`#session-liveness` 6/8).
    pub(crate) async fn restore_roster_on_resume(
        &self,
        os_user: &str,
        req: &ResumeSessionRequest,
        session_dir: &Path,
        worktree: &Path,
    ) -> Vec<RestoredTab> {
        let tab_jail_spawn = self.tab_jail_spawn();
        let subject = (self.user_resolver)(&req.session_token).unwrap_or_default();
        let tab_target_shell = self.tab_target_shell();
        restore_tab_roster(RosterRestore {
            session_id: &req.session_id,
            session_dir,
            worktree,
            os_user,
            subject: &subject,
            cli_manager: &self.cli_manager,
            coding_agents: &self.coding_agents,
            roster_store: self.cli_manager.tab_rosters(),
            tab_jails: &self.tab_jails,
            tab_jail_spawn: &*tab_jail_spawn,
            tab_target_shell: std::sync::Arc::clone(&tab_target_shell),
        })
        .await
    }
}
