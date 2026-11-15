//! The daemon's [`CoderWorkspaceSource`]: asks a Coder deployment for the workspaces a session
//! token's user owns.
//!
//! `GET <deployment root>/api/v2/workspaces?q=owner:me` with the token in the
//! `Coder-Session-Token` header. Made over HTTP rather than through the `coder` CLI, so a host
//! without the CLI still lists. Here rather than in `tddy-host-service` for the reason
//! [`crate::coder_deployment_probe`] is: the HTTP client is the daemon's.

use async_trait::async_trait;
use tddy_host_service::coder_workspaces::{
    CoderCredentials, CoderWorkspace, CoderWorkspaceListError, CoderWorkspaceSource,
};

/// [`CoderWorkspaceSource`] over `reqwest`.
pub struct HttpCoderWorkspaceSource {
    // TODO(coder-shell): implement — read by `workspaces`.
    #[allow(dead_code)]
    client: reqwest::Client,
}

impl HttpCoderWorkspaceSource {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl CoderWorkspaceSource for HttpCoderWorkspaceSource {
    /// Each entry of the answer's `workspaces[]`: `owner_name`, `name`, and `latest_build.status`
    /// as its status. A 401 is [`CoderWorkspaceListError::Rejected`] with the deployment's
    /// `message`; no answer is `Unreachable`; anything else is `Unusable`, naming what was seen.
    async fn workspaces(
        &self,
        credentials: &CoderCredentials,
    ) -> Result<Vec<CoderWorkspace>, CoderWorkspaceListError> {
        // TODO(coder-shell): implement
        let _ = credentials;
        Err(CoderWorkspaceListError::Unusable(
            "asking a Coder deployment for its workspaces is not implemented yet".to_string(),
        ))
    }
}
