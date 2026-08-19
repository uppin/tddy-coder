//! An assistant's name is the `--agent` value a session is started with, so the registry refuses
//! every name that already resolves to something else on this daemon — and every name `--agent`
//! could never match at all.
//!
//! `model_registry_store_unit.rs` covers the assistant CRUD itself; this suite covers only the
//! name space the assistant joins.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (Assistants).

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::model_registry::{ModelRegistryStore, NewAssistant, NewProvider};
use tddy_service::proto::models::ProviderKind;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_DAEMON: &str = "workstation-1";

/// The operator every row these tests write belongs to.
const THE_OPERATOR: &str = "testuser";

/// A registry holding one provider, reserving the ids this daemon's `allowed_agents` lists, next to
/// a `<tddyhome>/agents` directory the tests write defs into.
struct Registry {
    _dir: tempfile::TempDir,
    agents_dir: PathBuf,
    store: Arc<ModelRegistryStore>,
}

impl Registry {
    /// Write a `<tddyhome>/agents/<file_stem>.yaml` def resolvable as `--agent <def_name>`, and
    /// answer with the file it went into.
    ///
    /// The stem and the name are separate arguments because a def's name is the value *inside* the
    /// file: a guard that only looked at file names would miss half the defs an operator can write.
    fn given_an_agent_def(&self, def_name: &str, file_stem: &str) -> PathBuf {
        std::fs::create_dir_all(&self.agents_dir).expect("the agents directory must be creatable");
        let path = self.agents_dir.join(format!("{file_stem}.yaml"));
        std::fs::write(
            &path,
            format!(
                "name: {def_name}\nmodel: qwen3:32b\nbase_url: \"http://127.0.0.1:11434\"\n\
                 tools: [READ]\n"
            ),
        )
        .expect("the agent def must be writable");
        path
    }

    /// Write a `<tddyhome>/agents/<file_stem>.yaml` claiming `def_name` but naming no endpoint, so
    /// it does not load as a def and `--agent <def_name>` resolves to nothing.
    fn given_an_agent_def_file_naming_no_endpoint(&self, def_name: &str, file_stem: &str) {
        std::fs::create_dir_all(&self.agents_dir).expect("the agents directory must be creatable");
        std::fs::write(
            self.agents_dir.join(format!("{file_stem}.yaml")),
            format!("name: {def_name}\nmodel: qwen3:32b\n"),
        )
        .expect("the agent def must be writable");
    }

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
    let agents_dir = dir.path().join("agents");
    let store = Arc::new(
        ModelRegistryStore::open(&dir.path().join("models.db"), THIS_DAEMON, &agents_dir)
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
    Registry {
        _dir: dir,
        agents_dir,
        store,
    }
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
        Err("invalid name: 'cursor' is a coding backend".to_string())
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
        Err("invalid name: 'house-agent' is listed in this daemon's allowed_agents".to_string())
    );
}

#[tokio::test]
async fn a_name_an_agents_directory_def_already_answers_to_is_refused() {
    // Given a def an operator wrote under `<tddyhome>/agents`, in a file of another name
    let registry = a_registry_reserving(&[]).await;
    let def_file = registry.given_an_agent_def("explorer", "team-explorer");

    // When
    let outcome = registry.when_creating_an_assistant_named("explorer").await;

    // Then — refused, naming the file to edit; the registry wins on a tie, so admitting this name
    // would have stopped that def resolving with nothing said about it
    assert_eq!(
        outcome,
        Err(format!(
            "invalid name: 'explorer' is defined by the agent def {}",
            def_file.display()
        ))
    );
}

#[tokio::test]
async fn a_name_no_agents_directory_def_answers_to_is_accepted() {
    // Given an agents directory holding a def under a different name
    let registry = a_registry_reserving(&[]).await;
    registry.given_an_agent_def("explorer", "team-explorer");

    // When
    let outcome = registry.when_creating_an_assistant_named("reviewer").await;

    // Then
    assert_eq!(outcome, Ok(()));
}

#[tokio::test]
async fn a_name_claimed_only_by_a_def_file_that_does_not_load_is_accepted() {
    // Given a def file that names no endpoint, so `--agent explorer` resolves to nothing
    let registry = a_registry_reserving(&[]).await;
    registry.given_an_agent_def_file_naming_no_endpoint("explorer", "team-explorer");

    // When
    let outcome = registry.when_creating_an_assistant_named("explorer").await;

    // Then — a file nobody can start an agent from reserves no name
    assert_eq!(outcome, Ok(()));
}

#[tokio::test]
async fn a_second_assistant_of_the_same_name_is_a_duplicate_not_a_reserved_name() {
    // Given an assistant already holding the name
    let registry = a_registry_reserving(&[]).await;
    registry
        .when_creating_an_assistant_named("repo-explorer")
        .await
        .expect("the first assistant must be created");

    // When another is created under it
    let outcome = registry
        .when_creating_an_assistant_named("repo-explorer")
        .await;

    // Then it is `already exists` — there is a row to delete, unlike a name this daemon reserves
    assert!(
        outcome
            .as_ref()
            .expect_err("a duplicate name must be refused")
            .starts_with("already exists: an assistant named 'repo-explorer' already exists"),
        "got: {outcome:?}"
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
