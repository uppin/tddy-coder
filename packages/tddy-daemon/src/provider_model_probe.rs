//! The daemon's [`ProviderModelProbe`]: asks a provider endpoint for its model list over HTTPS.
//!
//! OpenAI-compatible endpoints answer `GET <openai root>/models` with a bearer key; an endpoint that
//! serves only Anthropic Messages answers `GET <anthropic root>/v1/models` with `x-api-key`. The root
//! is used as given — a root that already ends in `/v1` is not given a second one.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use tddy_accounts::{ProviderEndpoints, ProviderModelProbe};
use tddy_credentials::SecretString;

/// [`ProviderModelProbe`] over `reqwest`.
pub struct HttpProviderModelProbe {
    client: reqwest::Client,
}

impl HttpProviderModelProbe {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

/// The Anthropic API version header every Anthropic-protocol request carries.
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// How long a provider gets to list its models.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
/// How much of a refusal body is passed back to the person.
const REFUSAL_BODY_LIMIT: usize = 300;

#[async_trait]
impl ProviderModelProbe for HttpProviderModelProbe {
    async fn list_models(
        &self,
        endpoints: &ProviderEndpoints,
        api_key: &SecretString,
    ) -> Result<Vec<String>, String> {
        let openai_root = endpoints
            .openai_chat
            .as_deref()
            .or(endpoints.openai_responses.as_deref());
        let request = match (openai_root, endpoints.anthropic_messages.as_deref()) {
            (Some(root), _) => self
                .client
                .get(format!("{}/models", root.trim_end_matches('/')))
                .bearer_auth(api_key.expose()),
            (None, Some(root)) => self
                .client
                .get(format!("{}/v1/models", root.trim_end_matches('/')))
                .header("x-api-key", api_key.expose())
                .header("anthropic-version", ANTHROPIC_VERSION),
            (None, None) => return Err("the account serves no endpoint to list models from".into()),
        };
        let response = request
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|error| format!("the endpoint could not be reached: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let body: String = body.trim().chars().take(REFUSAL_BODY_LIMIT).collect();
            return Err(if body.is_empty() {
                status.to_string()
            } else {
                format!("{status}: {body}")
            });
        }
        let listing: ModelListing = response
            .json()
            .await
            .map_err(|error| format!("the endpoint's model list was not understood: {error}"))?;
        Ok(listing.data.into_iter().map(|model| model.id).collect())
    }
}

/// The `{ "data": [ { "id": … } ] }` shape both the OpenAI and the Anthropic list answer with.
#[derive(Deserialize)]
struct ModelListing {
    data: Vec<ListedModel>,
}

#[derive(Deserialize)]
struct ListedModel {
    id: String,
}
