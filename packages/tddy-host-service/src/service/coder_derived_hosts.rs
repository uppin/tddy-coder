use std::sync::Arc;

use super::HostServiceImpl;

use crate::coder_derived_hosts::CoderDerivedHosts;
use crate::host_tooling::CoderCliStatus;

impl HostServiceImpl {
    /// The Coder workspaces `subject` reaches from this daemon, as a derived-host source: this
    /// service's Coder ports, under the id this daemon is listed with in `ListKnownHosts`.
    ///
    /// Built per call, because the accounts are one signed-in person's while
    /// [`crate::derived_hosts::DerivedHostSource::list`] is asked with an OS user. `coder_cli` is
    /// the host's tooling reading for the `coder` executable.
    #[must_use]
    pub fn coder_derived_hosts(
        &self,
        subject: &str,
        coder_cli: CoderCliStatus,
    ) -> CoderDerivedHosts {
        CoderDerivedHosts::new(
            self.local_instance_id(),
            subject,
            Arc::clone(&self.coder_accounts),
            Arc::clone(&self.coder_credentials),
            Arc::clone(&self.coder_workspaces),
            coder_cli,
        )
    }
}
