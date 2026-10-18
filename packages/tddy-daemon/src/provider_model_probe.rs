//! The daemon's [`ProviderModelProbe`]: asks a provider endpoint for its model list over HTTPS.
//!
//! OpenAI-compatible endpoints answer `GET <openai root>/models` with a bearer key; an endpoint that
//! serves only Anthropic Messages answers `GET <anthropic root>/v1/models` with `x-api-key`. The root
//! is used as given — a root that already ends in `/v1` is not given a second one.

use async_trait::async_trait;
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

#[async_trait]
impl ProviderModelProbe for HttpProviderModelProbe {
    async fn list_models(
        &self,
        endpoints: &ProviderEndpoints,
        api_key: &SecretString,
    ) -> Result<Vec<String>, String> {
        let _ = (&self.client, endpoints, api_key);
        // TODO(accounts): implement
        todo!()
    }
}
