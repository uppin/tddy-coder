use crate::connection_service::session_acting_identity::SessionAccountAccess;
use crate::connection_service::session_acting_identity::SessionIdentity;
use tddy_projects::project_storage::ProjectData;

impl super::DaemonSessionHost {
    /// This host's row of `project_id`, or `None` — logged — when it cannot be read.
    pub(crate) fn project_row(
        &self,
        os_user: &str,
        session_id: &str,
        project_id: &str,
    ) -> Option<ProjectData> {
        // A client-supplied checkout belongs to no project; there is nothing to read and nothing
        // wrong, so it is not a refusal worth a warning.
        if project_id.trim().is_empty() {
            return None;
        }
        match super::service_util::find_registered_project(&self.tddy_data_dir, os_user, project_id)
        {
            Ok((_, project)) => Some(project),
            Err(status) => {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "session {session_id} has no account identity: its project could not be \
                     read: {}",
                    status.message()
                );
                None
            }
        }
    }

    /// What the session's vault reads go through: this daemon's vaults, how a session token names
    /// its owner, and the token the session was started with.
    pub(crate) fn session_account_access(&self, session_token: &str) -> SessionAccountAccess {
        SessionAccountAccess::new(
            self.credential_vaults(),
            self.user_resolver(),
            session_token,
        )
        .with_github_cli(self.github_cli.clone())
    }

    /// The identity a session of `project_id` is launched with: its commit pairs and the handler
    /// that answers its tools' `github-token`, from the source the session was started with (its
    /// recorded `github-source.yaml`) or, with none, this host's row of the project as it stands
    /// now. A project that cannot be read — logged — yields neither.
    pub(crate) fn session_identity(
        &self,
        os_user: &str,
        session_id: &str,
        project_id: &str,
        session_token: &str,
    ) -> SessionIdentity {
        let project = self.project_row(os_user, session_id, project_id);
        let access = self.session_account_access(session_token);
        let Some(sessions_base) = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            os_user,
            Some(&self.tddy_data_dir),
        ) else {
            log::warn!(
                target: "tddy_daemon::connection_service",
                "session {session_id} has no account identity: could not resolve sessions path"
            );
            return access.project_session_identity(session_id, None);
        };
        access.session_identity_in(
            session_id,
            os_user,
            &tddy_core::session_lifecycle::unified_session_dir_path(&sessions_base, session_id),
            project.as_ref(),
        )
    }
}
