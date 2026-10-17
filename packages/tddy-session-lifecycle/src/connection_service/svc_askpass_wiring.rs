use std::sync::Arc;

use crate::connection_service::DaemonSessionHost;

impl DaemonSessionHost {
    /// Install the endpoint a clone's `SSH_ASKPASS` helper asks for a key passphrase (builder).
    ///
    /// It must raise its prompts on the registry, and decrypt with the keypair, the host service
    /// streams them from — otherwise the prompt never reaches the browser.
    #[must_use]
    pub fn with_askpass_endpoint(
        mut self,
        endpoint: Arc<tddy_host_service::askpass::AskpassEndpoint>,
    ) -> Self {
        self.debug_assert_rpc_families_not_installed("with_askpass_endpoint");
        self.askpass = Some(endpoint);
        self
    }
}
