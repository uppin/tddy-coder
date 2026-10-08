use crate::connection_service::session_acting_identity::SessionAccountAccess;
use crate::connection_service::session_acting_identity::SessionIdentity;
use tddy_projects::project_storage::AccountAssignment;

impl super::DaemonSessionHost {
    /// The accounts `project_id` assigns, or `None` — logged — when its row cannot be read.
    pub(crate) fn project_account_assignments(
        &self,
        os_user: &str,
        session_id: &str,
        project_id: &str,
    ) -> Option<Vec<AccountAssignment>> {
        // A client-supplied checkout belongs to no project; there is nothing to read and nothing
        // wrong, so it is not a refusal worth a warning.
        if project_id.trim().is_empty() {
            return None;
        }
        match super::service_util::find_registered_project(&self.tddy_data_dir, os_user, project_id)
        {
            Ok((_, project)) => Some(project.accounts),
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
    }

    /// The identity a session of `project_id` is launched with: its commit pairs and the handler
    /// that answers its tools' `github-token`, over the project's assignments as they stand now.
    /// A project that cannot be read — logged — yields neither.
    pub(crate) fn session_identity(
        &self,
        os_user: &str,
        session_id: &str,
        project_id: &str,
        session_token: &str,
    ) -> SessionIdentity {
        let accounts = self.project_account_assignments(os_user, session_id, project_id);
        self.session_account_access(session_token)
            .session_identity(session_id, accounts.as_deref())
    }
}
