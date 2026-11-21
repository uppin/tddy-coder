//! A target session's **shell tab on the target** (`#session-liveness` 6/8): the session's target
//! resolved through the roster's [`TabTargetShell`](tddy_agent_launch::tab_target_shell::TabTargetShell)
//! as the requesting OS user, the target user's login shell launched there in the session's worktree
//! ([`tddy_agent_launch::remote_agent_launch::launch_tab_on_target`]) as a local tab terminal, and
//! recorded `[target, shell]` in the session's roster. A target that cannot be resolved or reached
//! fails the start with why, and nothing is recorded. A tab "On this host" of a target session runs
//! in the session's directory on this host — where its main's transport client runs — since its
//! worktree is on the target.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-target-shell-tab.md

use std::path::{Path, PathBuf};

use tddy_agent_launch::remote_agent_launch::{launch_tab_on_target, TargetTab};
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::tab_roster::{LaunchNode, TabEntry};
use tddy_rpc::Status;
use tddy_terminal_rpc::service::TerminalLaunch;

use super::CliManagerTerminalRoster;

impl CliManagerTerminalRoster {
    /// Start a shell tab of `session_id` on the session's target, in its worktree there, record it
    /// `[target, shell]`, and return its fresh terminal id.
    pub(super) async fn start_target_shell_tab(
        &self,
        session_id: &str,
        launch: &TerminalLaunch,
    ) -> Result<String, Status> {
        let Some(shells) = &self.target_shell else {
            return Err(Status::failed_precondition(
                "this daemon starts no tabs on a target",
            ));
        };
        let session_dir = self.session_dir(session_id)?;
        let target = shells.target_of(&session_dir, &launch.subject)?;
        let terminal_id = uuid::Uuid::now_v7().to_string();
        launch_tab_on_target(
            &self.manager,
            session_id,
            &terminal_id,
            TargetTab {
                shell: target.shell,
                worktree: &target.worktree,
                session_dir: &session_dir,
            },
        )
        .await
        .map_err(|error| Status::failed_precondition(error.to_string()))?;
        let chain = vec![LaunchNode::target(), LaunchNode::shell()];
        if let Err(error) = self
            .manager
            .tab_rosters()
            .record(&session_dir, TabEntry::new(&terminal_id, chain))
            .await
        {
            // An unrecorded tab could not be restored, and nothing would know to end it.
            self.manager.stop_terminal(session_id, &terminal_id).await;
            return Err(Status::internal(format!(
                "failed to record the tab: {error}"
            )));
        }
        Ok(terminal_id)
    }

    /// Where a tab "On this host" of the target session `session_id` runs: the session's directory
    /// on this host.
    pub(super) fn target_sessions_host_directory(
        &self,
        session_id: &str,
    ) -> Result<PathBuf, Status> {
        self.session_dir(session_id)
    }
}

/// Whether `session_id`'s record under `tddy_data_dir` names a target. An id that is not one path
/// segment, or a session with no readable record, names none.
pub(super) fn records_a_target(tddy_data_dir: &Path, session_id: &str) -> bool {
    validate_session_id_segment(session_id).is_ok()
        && tddy_core::read_session_metadata(&unified_session_dir_path(tddy_data_dir, session_id))
            .map(|metadata| metadata.target.is_some())
            .unwrap_or(false)
}
