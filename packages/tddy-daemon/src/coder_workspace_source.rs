//! The daemon's [`CoderWorkspaceSource`]: asks a Coder deployment for the workspaces a session
//! token's user owns.
//!
//! `GET <deployment root>/api/v2/workspaces?q=owner:me` with the token in the
//! `Coder-Session-Token` header. Made over HTTP rather than through the `coder` CLI, so a host
//! without the CLI still lists. Here rather than in `tddy-host-service` for the reason
//! [`crate::coder_deployment_probe`] is: the HTTP client is the daemon's.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use tddy_host_service::coder_workspaces::{
    CoderCredentials, CoderWorkspace, CoderWorkspaceListError, CoderWorkspaceSource,
};

/// [`CoderWorkspaceSource`] over `reqwest`.
pub struct HttpCoderWorkspaceSource {
    client: reqwest::Client,
}

impl HttpCoderWorkspaceSource {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

const LISTING_TIMEOUT: Duration = Duration::from_secs(20);
const REFUSAL_BODY_LIMIT: usize = 300;

#[async_trait]
impl CoderWorkspaceSource for HttpCoderWorkspaceSource {
    /// Each entry of the answer's `workspaces[]`: `owner_name`, `name`, and `latest_build.status`
    /// as its status. A 401 is [`CoderWorkspaceListError::Rejected`] with the deployment's
    /// `message`; no answer is `Unreachable`; anything else is `Unusable`, naming what was seen.
    async fn workspaces(
        &self,
        credentials: &CoderCredentials,
    ) -> Result<Vec<CoderWorkspace>, CoderWorkspaceListError> {
        let response = self
            .client
            .get(format!(
                "{}/api/v2/workspaces",
                credentials.url.trim_end_matches('/')
            ))
            .query(&[("q", "owner:me")])
            .header("Coder-Session-Token", credentials.token.expose())
            .timeout(LISTING_TIMEOUT)
            .send()
            .await
            .map_err(|error| CoderWorkspaceListError::Unreachable(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|error| format!("(its body could not be read: {error})"));
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CoderWorkspaceListError::Rejected(rejection_message(&body)));
        }
        if !status.is_success() {
            let body: String = body.trim().chars().take(REFUSAL_BODY_LIMIT).collect();
            return Err(CoderWorkspaceListError::Unusable(if body.is_empty() {
                status.to_string()
            } else {
                format!("{status}: {body}")
            }));
        }
        let answer: WorkspaceList = serde_json::from_str(&body)
            .map_err(|error| CoderWorkspaceListError::Unusable(format!("{status}: {error}")))?;
        Ok(answer
            .workspaces
            .into_iter()
            .map(|workspace| CoderWorkspace {
                owner: workspace.owner_name,
                name: workspace.name,
                status: workspace.latest_build.status,
            })
            .collect())
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
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| body.trim().chars().take(REFUSAL_BODY_LIMIT).collect())
}

/// `GET /api/v2/workspaces` as a Coder deployment answers it: only the fields this listing reads.
#[derive(Deserialize)]
struct WorkspaceList {
    workspaces: Vec<WorkspaceEntry>,
}

#[derive(Deserialize)]
struct WorkspaceEntry {
    owner_name: String,
    name: String,
    latest_build: LatestBuild,
}

#[derive(Deserialize)]
struct LatestBuild {
    status: String,
}
