//! The client for every cloud provider kind (OpenAI, Fireworks, Anthropic): `GET /v1/models` to
//! enumerate, and no residency at all.
//!
//! One client rather than one per vendor because the listing endpoint and its payload are the same
//! everywhere this registry supports; only how the key is presented differs, which is what
//! [`CredentialStyle`] carries. Anthropic in particular rejects `Authorization: Bearer` outright —
//! routing it through the bearer path would 401 every request and land the 401 body in the
//! provider's `enumeration_error`.

use async_trait::async_trait;
use serde::Deserialize;
use tddy_service::proto::models::{ModelEntry, ModelLoadState};

use super::error::{truncate_provider_detail, ModelRegistryError};
use super::labels::UNDETERMINABLE_LABEL;
use super::provider_client::ProviderClient;
use super::provider_http::ProviderHttp;

/// The `anthropic-version` every Anthropic API request must carry; the API refuses a request
/// without one rather than assuming the newest.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// How a provider expects its api key to be presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStyle {
    /// `Authorization: Bearer <key>` — OpenAI, Fireworks, and everything modelled on them.
    Bearer,
    /// `x-api-key: <key>` plus `anthropic-version` — Anthropic.
    AnthropicApiKey,
}

pub struct OpenAiCompatibleProviderClient {
    base_url: String,
    provider_id: String,
    daemon_instance_id: String,
    api_key: Option<String>,
    credential_style: CredentialStyle,
    http: reqwest::Client,
}

impl OpenAiCompatibleProviderClient {
    /// A provider that authenticates with a bearer token.
    pub fn new(
        base_url: &str,
        provider_id: &str,
        daemon_instance_id: &str,
        api_key: Option<String>,
    ) -> Self {
        Self::with_credential_style(
            base_url,
            provider_id,
            daemon_instance_id,
            api_key,
            CredentialStyle::Bearer,
        )
    }

    pub fn with_credential_style(
        base_url: &str,
        provider_id: &str,
        daemon_instance_id: &str,
        api_key: Option<String>,
        credential_style: CredentialStyle,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            provider_id: provider_id.to_string(),
            daemon_instance_id: daemon_instance_id.to_string(),
            api_key,
            credential_style,
            http: ProviderHttp::default().client(),
        }
    }

    /// The request with this provider's credential presented the way that provider expects it.
    /// Nothing is invented in a missing key's place.
    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let Some(key) = &self.api_key else {
            return request;
        };
        match self.credential_style {
            CredentialStyle::Bearer => request.bearer_auth(key),
            CredentialStyle::AnthropicApiKey => request
                .header("x-api-key", key)
                .header("anthropic-version", ANTHROPIC_VERSION),
        }
    }

    /// The refusal both residency operations answer with. Stated rather than attempted: a cloud
    /// provider has nothing to load, so issuing a request would be a round trip whose only
    /// possible outcome is a different error message.
    fn residency_unsupported(&self) -> ModelRegistryError {
        ModelRegistryError::UnsupportedOperation(format!(
            "{} has no notion of model residency; load and unload do not apply",
            self.base_url
        ))
    }
}

#[async_trait]
impl ProviderClient for OpenAiCompatibleProviderClient {
    async fn list_models(&self) -> Result<Vec<ModelEntry>, ModelRegistryError> {
        let url = format!("{}/v1/models", self.base_url);
        let response = self
            .authorized(self.http.get(&url))
            .send()
            .await
            .map_err(|e| ModelRegistryError::Provider(format!("{e}: {url}")))?;

        let status = response.status();
        let body = response.text().await.map_err(|e| {
            ModelRegistryError::Provider(format!("{url}: reading the response: {e}"))
        })?;
        if !status.is_success() {
            // Truncated because this message is persisted on the provider row and returned by
            // every `ListProviders`; a cloud provider's error page is not a few bytes.
            return Err(ModelRegistryError::Provider(format!(
                "{url}: HTTP {}: {}",
                status.as_u16(),
                truncate_provider_detail(&body)
            )));
        }
        let listing: ModelsResponse = serde_json::from_str(&body).map_err(|e| {
            ModelRegistryError::Provider(format!("{url}: unexpected response ({e})"))
        })?;

        Ok(listing
            .data
            .into_iter()
            .map(|entry| ModelEntry {
                model_id: entry.id.clone(),
                provider_id: self.provider_id.clone(),
                label: entry.id,
                // `/v1/models` reports no capabilities, so there is nothing to derive a label
                // from. TODO: derive labels per provider kind once a kind-specific metadata
                // endpoint is wired (Fireworks and OpenAI both expose one).
                labels: vec![UNDETERMINABLE_LABEL.to_string()],
                load_state: ModelLoadState::Unsupported as i32,
                daemon_instance_id: self.daemon_instance_id.clone(),
                size_bytes: 0,
            })
            .collect())
    }

    async fn load_state(&self, _model_id: &str) -> Result<ModelLoadState, ModelRegistryError> {
        Ok(ModelLoadState::Unsupported)
    }

    async fn load(&self, _model_id: &str) -> Result<(), ModelRegistryError> {
        Err(self.residency_unsupported())
    }

    async fn unload(&self, _model_id: &str) -> Result<(), ModelRegistryError> {
        Err(self.residency_unsupported())
    }
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelListing>,
}

#[derive(Debug, Deserialize)]
struct ModelListing {
    id: String,
}
