use tddy_rpc::Status;

use std::path::Path;

use crate::split_ports::SplitSessions;

impl SplitSessions {
    /// Build the jail a sandboxed `workspace` session runs its tools in, and register it under the
    /// session id every later dispatch looks it up by
    /// (`docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox).
    ///
    /// A host with no sandbox backend, or a jail that will not come up, is an error — never a start
    /// that succeeds unconfined.
    pub async fn provision_workspace_tool_sandbox(
        &self,
        sessions_base: &Path,
        session_id: &str,
    ) -> Result<(), Status> {
        let spec = crate::workspace_session::workspace_sandbox_spec(sessions_base, session_id)?;
        let jail = self
            .workspace_sandbox_provisioner
            .provision(&spec)
            .await
            .map_err(tddy_daemon_sandbox::sandbox_session::sandbox_error_to_status)?;
        self.workspace_sandboxes
            .insert(session_id.to_string(), jail)
            .await;
        log::info!(
            "StartSession: workspace session {session_id} runs its tools in a jail holding {}",
            spec.worktree_path.display()
        );
        Ok(())
    }
}
