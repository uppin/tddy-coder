//! The port every provider kind is reached through, and the factory the service resolves a
//! provider row to a client with.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_service::proto::models::{ModelEntry, ModelLoadState, ProviderEntry};

use super::error::ModelRegistryError;

/// One provider endpoint, as the registry uses it.
///
/// Every method may fail, and a failure is always reported: an unreachable endpoint must never
/// look like "this provider offers no models" or "the model is not resident".
#[async_trait]
pub trait ProviderClient: Send + Sync {
    /// Everything the provider currently offers. Errors rather than returning a partial or empty
    /// catalog when the endpoint cannot be read.
    async fn list_models(&self) -> Result<Vec<ModelEntry>, ModelRegistryError>;

    /// Whether the provider currently holds `model_id` in memory.
    /// [`ModelLoadState::Unsupported`] for a provider kind with no notion of residency.
    async fn load_state(&self, model_id: &str) -> Result<ModelLoadState, ModelRegistryError>;

    /// Make `model_id` resident. [`ModelRegistryError::UnsupportedOperation`] where residency has
    /// no meaning.
    async fn load(&self, model_id: &str) -> Result<(), ModelRegistryError>;

    /// Evict `model_id`. [`ModelRegistryError::UnsupportedOperation`] where residency has no
    /// meaning.
    async fn unload(&self, model_id: &str) -> Result<(), ModelRegistryError>;
}

/// Resolves a stored provider row (plus its credential, when it has one) to a live client. A port
/// so the service's own rules can be exercised without HTTP.
///
/// Fallible on purpose: the row carries the provider kind as an integer, and a kind this build
/// does not know is not something to guess at. Guessing means handing the stored api key to
/// whichever client happened to be the catch-all.
pub trait ProviderClientFactory: Send + Sync {
    fn client_for(
        &self,
        provider: &ProviderEntry,
        credential: Option<String>,
    ) -> Result<Arc<dyn ProviderClient>, ModelRegistryError>;
}
