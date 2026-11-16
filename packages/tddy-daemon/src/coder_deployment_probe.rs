//! The daemon's [`CoderDeploymentProbe`]: asks a Coder deployment who a session token belongs to.
//!
//! `GET <deployment root>/api/v2/users/me` with the token in the `Coder-Session-Token` header —
//! the call the `coder` CLI makes on `coder login`. Made over HTTP rather than through that CLI, so
//! connecting an account needs no binary on the daemon host.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use tddy_accounts::{CoderDeploymentProbe, CoderDeploymentUrl, CoderProbeError, CoderUser};
use tddy_credentials::SecretString;

/// [`CoderDeploymentProbe`] over `reqwest`.
pub struct HttpCoderDeploymentProbe {
    client: reqwest::Client,
}

impl HttpCoderDeploymentProbe {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
const REFUSAL_BODY_LIMIT: usize = 300;

#[async_trait]
impl CoderDeploymentProbe for HttpCoderDeploymentProbe {
    async fn whoami(
        &self,
        url: &CoderDeploymentUrl,
        token: &SecretString,
    ) -> Result<CoderUser, CoderProbeError> {
        let response = self
            .client
            .get(format!(
                "{}/api/v2/users/me",
                url.as_str().trim_end_matches('/')
            ))
            .header("Coder-Session-Token", token.expose())
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|error| CoderProbeError::Unreachable(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|error| format!("(its body could not be read: {error})"));
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CoderProbeError::Rejected(rejection_message(&body)));
        }
        if !status.is_success() {
            let body: String = body.trim().chars().take(REFUSAL_BODY_LIMIT).collect();
            return Err(CoderProbeError::Unusable(if body.is_empty() {
                status.to_string()
            } else {
                format!("{status}: {body}")
            }));
        }
        let user: DeploymentUser = serde_json::from_str(&body)
            .map_err(|error| CoderProbeError::Unusable(format!("{status}: {error}")))?;
        Ok(CoderUser {
            id: user.id,
            username: user.username,
        })
    }
}

/// The deployment's reason for refusing a token: its JSON `message`, or the body itself.
fn rejection_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(|message| message.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.trim().to_string())
}

#[derive(Deserialize)]
struct DeploymentUser {
    id: String,
    username: String,
}
