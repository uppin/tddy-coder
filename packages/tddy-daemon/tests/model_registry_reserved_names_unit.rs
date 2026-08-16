//! An assistant's name is the `--agent` value a session is started with, so the registry refuses
//! every name that already resolves to something else on this daemon — and every name `--agent`
//! could never match at all.
//!
//! `model_registry_store_unit.rs` covers the assistant CRUD itself; this suite covers only the
//! name space the assistant joins.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (Assistants).

use std::sync::Arc;

use tddy_daemon::model_registry::{ModelRegistryStore, NewAssistant, NewProvider};
use tddy_service::proto::models::ProviderKind;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_DAEMON: &str = "workstation-1";

/// The operator every row these tests write belongs to.
const THE_OPERATOR: &str = "testuser";

/// A registry holding one provider, reserving the ids this daemon's `allowed_agents` lists.
struct Registry {
    _dir: tempfile::TempDir,
    store: Arc<ModelRegistryStore>,
}

impl Registry {
    async fn when_creating_an_assistant_named(&self, name: &str) -> Result<(), String> {
        self.store
            .create_assistant(
                NewAssistant {
                    name: name.to_string(),
                    label: String::new(),
                    provider_id: "prov-ollama".to_string(),
                    model_id: "qwen3:32b".to_string(),
                    system_prompt: String::new(),
                    tools: vec!["Read".to_string()],
                },
                THE_OPERATOR,
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

async fn a_registry_reserving(allowed_agent_ids: &[&str]) -> Registry {
    let dir = tempfile::tempdir().expect("a tempdir for the registry db");
    let store = Arc::new(
        ModelRegistryStore::open(&dir.path().join("models.db"), THIS_DAEMON)
            .await
            .expect("open the registry store")
            .reserving_agent_ids(allowed_agent_ids.iter().map(|id| id.to_string())),
    );
    store
        .create_provider(
            NewProvider {
                kind: ProviderKind::Ollama,
                label: "Workstation Ollama".to_string(),
                base_url: "http://127.0.0.1:11434".to_string(),
                api_key: None,
            },
            THE_OPERATOR,
        )
        .await
        .expect("the provider must be created");
    Registry { _dir: dir, store }
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_name_free_on_this_daemon_is_accepted() {
    // Given
    let registry = a_registry_reserving(&["claude"]).await;

    // When
    let outcome = registry
        .when_creating_an_assistant_named("repo-explorer")
        .await;

    // Then
    assert_eq!(outcome, Ok(()));
}

#[tokio::test]
async fn a_coding_backend_id_is_refused() {
    // Given
    let registry = a_registry_reserving(&[]).await;

    // When
    let outcome = registry.when_creating_an_assistant_named("cursor").await;

    // Then
    assert_eq!(
        outcome,
        Err("already exists: 'cursor' is a coding backend".to_string())
    );
}

#[tokio::test]
async fn an_allowed_agents_config_id_is_refused() {
    // Given — an operator-defined backend id, not one of the hardcoded ones
    let registry = a_registry_reserving(&["house-agent"]).await;

    // When
    let outcome = registry
        .when_creating_an_assistant_named("house-agent")
        .await;

    // Then
    assert_eq!(
        outcome,
        Err("already exists: 'house-agent' is listed in this daemon's allowed_agents".to_string())
    );
}

#[tokio::test]
async fn a_builtin_agent_def_name_is_refused() {
    // Given
    let registry = a_registry_reserving(&[]).await;

    // When
    let outcome = registry
        .when_creating_an_assistant_named("fastcontext")
        .await;

    // Then
    assert_eq!(
        outcome,
        Err("already exists: 'fastcontext' is a coding backend".to_string())
    );
}

#[tokio::test]
async fn an_empty_name_is_refused() {
    // Given
    let registry = a_registry_reserving(&[]).await;

    // When
    let outcome = registry.when_creating_an_assistant_named("").await;

    // Then
    assert_eq!(
        outcome,
        Err("invalid name: an assistant name is required".to_string())
    );
}

#[tokio::test]
async fn a_whitespace_only_name_is_refused() {
    // Given
    let registry = a_registry_reserving(&[]).await;

    // When
    let outcome = registry.when_creating_an_assistant_named("   ").await;

    // Then
    assert_eq!(
        outcome,
        Err("invalid name: an assistant name is required".to_string())
    );
}

#[tokio::test]
async fn a_name_padded_with_whitespace_is_refused() {
    // Given
    let registry = a_registry_reserving(&[]).await;

    // When
    let outcome = registry
        .when_creating_an_assistant_named(" repo-explorer")
        .await;

    // Then
    assert_eq!(
        outcome,
        Err(
            "invalid name: ' repo-explorer' has leading or trailing whitespace; `--agent` would \
             never match it"
                .to_string()
        )
    );
}
