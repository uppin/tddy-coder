//! The per-daemon model registry: the providers this daemon talks to, the models they offer, and
//! the assistants composed from those models.
//!
//! The registry is one daemon's own SQLite database; nothing here forwards to a peer. The web fans
//! out to each common-room daemon and merges, exactly as the sessions drawer does.
//!
//! See docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md.

pub mod acp_service;
pub mod assistant_def;
pub mod error;
pub mod labels;
pub mod ollama;
pub mod openai_compatible;
pub mod provider_client;
pub mod provider_http;
pub mod service;
pub mod store;
pub mod tool_dispatcher;

pub use acp_service::ModelAcpService;
pub use assistant_def::{assistant_to_agent_def, registry_agent_defs};
pub use error::{truncate_provider_detail, ModelRegistryError, MAX_PROVIDER_DETAIL_BYTES};
pub use labels::{capabilities_to_labels, UNDETERMINABLE_LABEL};
pub use ollama::OllamaProviderClient;
pub use openai_compatible::{CredentialStyle, OpenAiCompatibleProviderClient};
pub use provider_client::{ProviderClient, ProviderClientFactory};
pub use provider_http::ProviderHttp;
pub use service::{DefaultProviderClients, ModelRegistryServiceImpl};
pub use store::{ModelRegistryStore, NewAssistant, NewProvider};
pub use tool_dispatcher::EngineToolDispatcher;
