//! The daemon's [`CoderDeploymentProbe`]: asks a Coder deployment who a session token belongs to.
//!
//! `GET <deployment root>/api/v2/users/me` with the token in the `Coder-Session-Token` header —
//! the call the `coder` CLI makes on `coder login`. Made over HTTP rather than through that CLI, so
//! connecting an account needs no binary on the daemon host.

use async_trait::async_trait;
use tddy_accounts::{CoderDeploymentProbe, CoderDeploymentUrl, CoderProbeError, CoderUser};
use tddy_credentials::SecretString;

/// [`CoderDeploymentProbe`] over `reqwest`.
pub struct HttpCoderDeploymentProbe {
    // TODO(coder-account): implement — read by `whoami`.
    #[allow(dead_code)]
    client: reqwest::Client,
}

impl HttpCoderDeploymentProbe {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl CoderDeploymentProbe for HttpCoderDeploymentProbe {
    async fn whoami(
        &self,
        url: &CoderDeploymentUrl,
        token: &SecretString,
    ) -> Result<CoderUser, CoderProbeError> {
        // TODO(coder-account): implement
        let _ = (url, token);
        Err(CoderProbeError::Unusable(
            "asking a Coder deployment who a token belongs to is not implemented yet".to_string(),
        ))
    }
}
