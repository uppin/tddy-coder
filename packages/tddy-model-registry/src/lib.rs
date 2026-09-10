//! The daemon's model registry: provider-backed chat models, registry assistants, and the ACP
//! bridge that lets one be addressed as an agent.
//!
//! The registry is one daemon's own SQLite database; nothing here forwards to a peer. The web fans
//! out to each common-room daemon and merges, exactly as the sessions drawer does.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 2. It was **the cleanest extraction in that
//! crate** and was chosen as an early node for exactly that reason: already directory-shaped as
//! `model_registry/`, **zero** outbound `crate::` edges beyond its own directory, and **zero**
//! inline `#[cfg(test)]` lines — all of its tests are integration tests in dedicated files, so they
//! move with the code rather than being rewritten.
//!
//! Its two proto services, `models.ModelRegistryService` and `acp.AcpService`, were **already their
//! own protos**. Nothing about the wire changes here and no client migrates; only the crate the
//! implementation lives in.
//!
//! `sqlx`, `agent-client-protocol` and `tddy-acp` are attributable to this subsystem alone and
//! leave `tddy-daemon` with it.
//!
//! See docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md.

use std::sync::Arc;

use tddy_daemon_kernel::SessionUserResolver;
use tddy_task::TaskRegistry;

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
pub mod workspace;

pub use acp_service::ModelAcpService;
pub use assistant_def::{
    assistant_to_agent_def, registry_agent_def_with_credential, registry_agent_defs,
};
pub use error::{truncate_provider_detail, ModelRegistryError, MAX_PROVIDER_DETAIL_BYTES};
pub use labels::{capabilities_to_labels, reported_capabilities_to_labels, UNDETERMINABLE_LABEL};
pub use ollama::OllamaProviderClient;
pub use openai_compatible::{CredentialStyle, OpenAiCompatibleProviderClient};
pub use provider_client::{ProviderClient, ProviderClientFactory};
pub use provider_http::ProviderHttp;
pub use service::{DefaultProviderClients, ModelRegistryServiceImpl};
pub use store::{ModelRegistryStore, NewAssistant, NewProvider, MAX_SYSTEM_PROMPT_BYTES};
pub use tool_dispatcher::EngineToolDispatcher;
pub use workspace::{resolve_chat_workspace, ChatWorkspaceRoots};

/// The `models.ModelRegistryService` entry the daemon's wiring layer registers.
///
/// A subsystem crate's whole contract with the wiring layer is to produce a
/// [`tddy_rpc::ServiceEntry`]; nothing else about the daemon's assembly needs to know this crate
/// exists.
pub fn build_model_registry_entry(
    store: Arc<ModelRegistryStore>,
    clients: Arc<dyn ProviderClientFactory>,
    user_resolver: SessionUserResolver,
) -> tddy_rpc::ServiceEntry {
    let server = tddy_service::ModelRegistryServiceServer::new(ModelRegistryServiceImpl::new(
        store,
        clients,
        user_resolver,
    ));
    tddy_rpc::ServiceEntry {
        name: "models.ModelRegistryService",
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}

/// The `acp.AcpService` entry — a registry model or assistant addressed as an ACP agent.
///
/// The *session*-addressed `acp.AcpService` is mounted per session process; this one is the
/// daemon's own, so the Models & Agents screen can open a chat without a session existing at all.
pub fn build_model_acp_entry(
    store: Arc<ModelRegistryStore>,
    tasks: TaskRegistry,
    user_resolver: SessionUserResolver,
    workspace_roots: ChatWorkspaceRoots,
) -> tddy_rpc::ServiceEntry {
    let server = tddy_service::AcpServiceServer::new(ModelAcpService::new(
        store,
        tasks,
        user_resolver,
        workspace_roots,
    ));
    tddy_rpc::ServiceEntry {
        name: tddy_service::AcpServiceServer::<ModelAcpService>::NAME,
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store on a scratch database, which is all either entry constructor needs of it.
    async fn a_store(root: &std::path::Path) -> Arc<ModelRegistryStore> {
        Arc::new(
            ModelRegistryStore::open(&root.join("models.db"), "instance-1", &root.join("agents"))
                .await
                .expect("open the registry"),
        )
    }

    #[tokio::test]
    async fn names_the_service_the_wiring_layer_registers() {
        // Given a store, the provider clients it talks through, and the token resolver
        let root = tempfile::tempdir().unwrap();
        let store = a_store(root.path()).await;
        let clients: Arc<dyn ProviderClientFactory> = Arc::new(DefaultProviderClients);
        let user_resolver: SessionUserResolver = Arc::new(|_| None);

        // When
        let entry = build_model_registry_entry(store, clients, user_resolver);

        // Then
        assert_eq!(entry.name, "models.ModelRegistryService");
    }

    #[tokio::test]
    async fn names_the_acp_service_the_wiring_layer_registers() {
        // Given
        let root = tempfile::tempdir().unwrap();
        let store = a_store(root.path()).await;
        let user_resolver: SessionUserResolver = Arc::new(|_| None);
        let workspace_roots: ChatWorkspaceRoots = Arc::new(|_| Ok(Vec::new()));

        // When
        let entry =
            build_model_acp_entry(store, TaskRegistry::new(), user_resolver, workspace_roots);

        // Then — the coordinate `tddy/acp/v1/acp.proto` declares, which is what a browser dials.
        // The changeset writes it as `acp.AcpService`; that is prose shorthand, and the registered
        // name is the fully-qualified one.
        assert_eq!(entry.name, "tddy.acp.v1.AcpService");
    }

    /// A daemon whose registry is unreadable must say so rather than answer "no assistants": an
    /// empty list is a fabricated answer an operator acts on.
    #[tokio::test]
    async fn refuses_to_open_a_registry_it_cannot_read() {
        // Given a path that is a directory, not a database
        let root = tempfile::tempdir().unwrap();

        // When
        let outcome =
            ModelRegistryStore::open(root.path(), "instance-1", &root.path().join("agents")).await;

        // Then
        assert!(
            matches!(outcome, Err(ModelRegistryError::Storage(_))),
            "an unreadable registry is an error, never an empty answer"
        );
    }
}
