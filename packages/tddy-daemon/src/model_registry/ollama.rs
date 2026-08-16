//! The Ollama provider client.
//!
//! Ollama is a first-class provider kind rather than "an OpenAI-compatible endpoint" because only
//! its own API answers the two questions the Models screen exists to ask: what has this host
//! pulled (`/api/tags` + `/api/show`), and what is resident right now (`/api/ps`, and
//! `/api/generate` with `keep_alive` to change it).

use async_trait::async_trait;
use serde::Deserialize;
use tddy_service::proto::models::{ModelEntry, ModelLoadState};

use super::error::ModelRegistryError;
use super::labels::capabilities_to_labels;
use super::provider_client::ProviderClient;
use super::provider_http::{decode, ProviderHttp};

/// How long a loaded model stays resident before Ollama evicts it on its own. Sent as
/// `keep_alive` on the zero-token generate that loads it.
const LOAD_KEEP_ALIVE: &str = "10m";

pub struct OllamaProviderClient {
    base_url: String,
    provider_id: String,
    daemon_instance_id: String,
    /// Ollama itself takes no api key, but an Ollama published to a network almost always sits
    /// behind something that does (a reverse proxy, an access gateway, Ollama's own hosted tier).
    /// The registry's provider form offers the field and reports `has_credential: true` once it is
    /// filled, so the key has to actually be sent — the alternative, refusing a credential on an
    /// `OLLAMA` provider, would leave every proxied deployment unusable.
    api_key: Option<String>,
    http_config: ProviderHttp,
    http: reqwest::Client,
}

impl OllamaProviderClient {
    pub fn new(
        base_url: &str,
        provider_id: &str,
        daemon_instance_id: &str,
        api_key: Option<String>,
    ) -> Self {
        let http_config = ProviderHttp::default();
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            provider_id: provider_id.to_string(),
            daemon_instance_id: daemon_instance_id.to_string(),
            api_key,
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

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// The request with this provider's credential on it, when it has one. Nothing is invented in
    /// a missing key's place.
    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => request.bearer_auth(key),
            None => request,
        }
    }

    /// The capability tokens `/api/show` reports for one model.
    async fn capabilities(&self, model_id: &str) -> Result<Vec<String>, ModelRegistryError> {
        let response = self
            .authorized(
                self.http
                    .post(self.url("/api/show"))
                    .json(&serde_json::json!({ "model": model_id })),
            )
            .send()
            .await
            .map_err(|e| self.unreachable("/api/show", e))?;
        let show: ShowResponse = decode(response, &self.url("/api/show")).await?;
        Ok(show.capabilities)
    }

    /// The model names Ollama currently holds in memory.
    async fn resident_models(&self) -> Result<Vec<String>, ModelRegistryError> {
        let response = self
            .authorized(self.http.get(self.url("/api/ps")))
            .send()
            .await
            .map_err(|e| self.unreachable("/api/ps", e))?;
        let ps: PsResponse = decode(response, &self.url("/api/ps")).await?;
        Ok(ps.models.into_iter().map(|m| m.name).collect())
    }

    /// A zero-token generate whose only purpose is the `keep_alive` it carries: a duration loads
    /// the model, `0` evicts it.
    async fn set_keep_alive(
        &self,
        model_id: &str,
        keep_alive: serde_json::Value,
    ) -> Result<(), ModelRegistryError> {
        let response = self
            .authorized(
                self.http
                    .post(self.url("/api/generate"))
                    .json(&serde_json::json!({
                        "model": model_id,
                        "keep_alive": keep_alive,
                    })),
            )
            .send()
            .await
            .map_err(|e| self.unreachable("/api/generate", e))?;
        let _: serde_json::Value = decode(response, &self.url("/api/generate")).await?;
        Ok(())
    }

    fn unreachable(&self, path: &str, error: reqwest::Error) -> ModelRegistryError {
        super::provider_http::unreachable(&self.url(path), error)
    }
}

#[async_trait]
impl ProviderClient for OllamaProviderClient {
    /// The whole catalog, under one budget.
    ///
    /// Enumeration is `/api/tags`, `/api/ps`, and then one `/api/show` per pulled model — a host
    /// with a large library costs dozens of round trips inside a single `RefreshProviderModels`.
    /// Each has its own request timeout; the budget bounds the walk as a whole, so the RPC answers
    /// either way.
    async fn list_models(&self) -> Result<Vec<ModelEntry>, ModelRegistryError> {
        self.http_config
            .within_enumeration_budget(&self.base_url, self.enumerate())
            .await
    }

    async fn load_state(&self, model_id: &str) -> Result<ModelLoadState, ModelRegistryError> {
        let resident = self.resident_models().await?;
        Ok(if resident.iter().any(|name| name == model_id) {
            ModelLoadState::Loaded
        } else {
            ModelLoadState::NotLoaded
        })
    }

    async fn load(&self, model_id: &str) -> Result<(), ModelRegistryError> {
        self.set_keep_alive(model_id, serde_json::json!(LOAD_KEEP_ALIVE))
            .await
    }

    async fn unload(&self, model_id: &str) -> Result<(), ModelRegistryError> {
        self.set_keep_alive(model_id, serde_json::json!(0)).await
    }
}

impl OllamaProviderClient {
    /// What the host has pulled, each model labelled from its own `/api/show` and cross-referenced
    /// against `/api/ps` for residency.
    async fn enumerate(&self) -> Result<Vec<ModelEntry>, ModelRegistryError> {
        let response = self
            .authorized(self.http.get(self.url("/api/tags")))
            .send()
            .await
            .map_err(|e| self.unreachable("/api/tags", e))?;
        let tags: TagsResponse = decode(response, &self.url("/api/tags")).await?;
        let resident = self.resident_models().await?;

        let mut models = Vec::with_capacity(tags.models.len());
        for tag in tags.models {
            let labels = capabilities_to_labels(&self.capabilities(&tag.name).await?);
            let load_state = if resident.iter().any(|name| name == &tag.name) {
                ModelLoadState::Loaded
            } else {
                ModelLoadState::NotLoaded
            };
            models.push(ModelEntry {
                model_id: tag.name.clone(),
                provider_id: self.provider_id.clone(),
                label: tag.name,
                labels,
                load_state: load_state as i32,
                daemon_instance_id: self.daemon_instance_id.clone(),
                size_bytes: tag.size,
            });
        }
        Ok(models)
    }
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagEntry>,
}

#[derive(Debug, Deserialize)]
struct TagEntry {
    name: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct ShowResponse {
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PsResponse {
    #[serde(default)]
    models: Vec<PsEntry>,
}

#[derive(Debug, Deserialize)]
struct PsEntry {
    name: String,
}
