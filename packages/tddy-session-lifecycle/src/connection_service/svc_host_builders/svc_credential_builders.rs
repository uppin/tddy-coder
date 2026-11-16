use crate::connection_service::DaemonSessionHost;
use std::sync::Arc;

impl DaemonSessionHost {
    /// Act on the operator's own GitHub credential for PR-status reads (builder). The vaults are
    /// the ones the auth service opens at login and reopens at refresh; without them, PR status
    /// reports itself unavailable.
    pub fn with_credential_vaults(mut self, vaults: Arc<tddy_daemon_auth::SessionVaults>) -> Self {
        self.debug_assert_rpc_families_not_installed("with_credential_vaults");
        self.credential_vaults = Some(vaults);
        self
    }

    /// Ask this `gh` for a Local gh project's identity and token (builder), instead of the one on
    /// the spawned child's `PATH`.
    pub fn with_github_cli(mut self, github_cli: tddy_host_service::GithubCli) -> Self {
        self.github_cli = github_cli;
        self
    }

    /// Read the Coder account of a session whose target is a Coder workspace from `credentials`
    /// (builder) — the daemon's vaults — at its start and at every resume.
    pub fn with_coder_credentials(
        mut self,
        credentials: Arc<dyn tddy_host_service::coder_workspaces::CoderCredentialSource>,
    ) -> Self {
        self.coder_credentials = credentials;
        self
    }
}
