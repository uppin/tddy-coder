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

use super::error::ModelRegistryError;
use super::labels::reported_capabilities_to_labels;
use super::provider_client::ProviderClient;
use super::provider_http::{decode, unreachable, ProviderHttp};

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
    http_config: ProviderHttp,
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
        let http_config = ProviderHttp::default();
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            provider_id: provider_id.to_string(),
            daemon_instance_id: daemon_instance_id.to_string(),
            api_key,
            credential_style,
            http: http_config.client(),
            http_config,
        }
    }

    /// Talk to this provider under a different transport budget than [`ProviderHttp::default`].
    pub fn with_http_config(mut self, http_config: ProviderHttp) -> Self {
        self.http = http_config.client();
        self.http_config = http_config;
        self
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
        self.http_config
            .within_enumeration_budget(&self.base_url, self.enumerate(&url))
            .await
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

impl OpenAiCompatibleProviderClient {
    /// The provider's catalog, each entry labelled from whatever that entry itself reports.
    async fn enumerate(&self, url: &str) -> Result<Vec<ModelEntry>, ModelRegistryError> {
        let response = self
            .authorized(self.http.get(url))
            .send()
            .await
            .map_err(|e| unreachable(url, e))?;
        let listing: ModelsResponse = decode(response, url).await?;

        Ok(listing
            .data
            .into_iter()
            .map(|entry| ModelEntry {
                model_id: entry.id.clone(),
                provider_id: self.provider_id.clone(),
                label: entry.id,
                labels: reported_capabilities_to_labels(
                    entry.supports_chat,
                    entry.supports_tools,
                    entry.supports_image_input,
                ),
                load_state: ModelLoadState::Unsupported as i32,
                daemon_instance_id: self.daemon_instance_id.clone(),
                size_bytes: 0,
            })
            .collect())
    }
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelListing>,
}

/// One `/v1/models` entry.
///
/// The three capability flags are optional because only some providers report them: Fireworks
/// answers `supports_chat` / `supports_tools` / `supports_image_input` per model, while OpenAI's
/// own `/v1/models` entry carries nothing but `id`, `created` and `owned_by`. Absent means "this
/// provider did not say", which [`reported_capabilities_to_labels`] renders as `"unknown"` rather
/// than as a guess.
#[derive(Debug, Deserialize)]
struct ModelListing {
    id: String,
    #[serde(default)]
    supports_chat: Option<bool>,
    #[serde(default)]
    supports_tools: Option<bool>,
    #[serde(default)]
    supports_image_input: Option<bool>,
}
