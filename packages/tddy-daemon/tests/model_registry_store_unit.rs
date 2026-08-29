//! The per-daemon model registry: its SQLite store, the capability-label derivation, and the
//! projection of an assistant onto a `SpecializedAgentDef`.
//!
//! The store follows the `session_catalog` precedent (`tddy-core/src/session_catalog/store.rs`):
//! `sqlx` runtime query API, WAL journal, created on demand. These tests run against a real DB file
//! in a tempdir — a fake would prove nothing about the schema.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md.

use tddy_daemon::model_registry::{
    assistant_to_agent_def, capabilities_to_labels, registry_agent_def_with_credential,
    reported_capabilities_to_labels, truncate_provider_detail, ModelRegistryError,
    ModelRegistryStore, NewAssistant, NewProvider, MAX_PROVIDER_DETAIL_BYTES,
    MAX_SYSTEM_PROMPT_BYTES,
};
use tddy_discovery::agent_def::SubagentTool;
use tddy_service::proto::models::{ModelEntry, ModelLoadState, ProviderEntry, ProviderKind};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_DAEMON: &str = "workstation-1";

/// The operator every row here belongs to. Reads are fleet-wide, writes are the owner's, so a
/// fixture that writes has to say who is writing.
const AN_OPERATOR: &str = "alice";

/// A second operator on the same daemon — the one whose reach into the first's rows is the point
/// of the ownership rules.
const ANOTHER_OPERATOR: &str = "bob";

/// A store over a fresh DB file. The tempdir is returned so it outlives the store.
async fn a_store() -> (tempfile::TempDir, ModelRegistryStore) {
    let dir = tempfile::tempdir().expect("a tempdir for the registry db");
    let store = ModelRegistryStore::open(
        &dir.path().join("models.db"),
        THIS_DAEMON,
        &dir.path().join("agents"),
    )
    .await
    .expect("open the registry store");
    (dir, store)
}

fn a_keyless_ollama_provider() -> NewProvider {
    NewProvider {
        kind: ProviderKind::Ollama,
        label: "Local Ollama".to_string(),
        base_url: "http://localhost:11434".to_string(),
        api_key: None,
    }
}

fn a_credentialed_cloud_provider() -> NewProvider {
    NewProvider {
        kind: ProviderKind::Fireworks,
        label: "Fireworks".to_string(),
        base_url: "https://api.fireworks.ai/inference".to_string(),
        api_key: Some("fw-secret-key".to_string()),
    }
}

fn a_model(provider_id: &str, model_id: &str, labels: &[&str]) -> ModelEntry {
    ModelEntry {
        model_id: model_id.to_string(),
        provider_id: provider_id.to_string(),
        label: model_id.to_string(),
        labels: labels.iter().map(|s| s.to_string()).collect(),
        load_state: ModelLoadState::NotLoaded as i32,
        daemon_instance_id: THIS_DAEMON.to_string(),
        size_bytes: 1_000,
    }
}

fn an_assistant(provider_id: &str) -> NewAssistant {
    NewAssistant {
        name: "repo-reader".to_string(),
        label: "Repo Reader".to_string(),
        provider_id: provider_id.to_string(),
        model_id: "qwen3:32b".to_string(),
        system_prompt: "You read code and answer questions about it.".to_string(),
        tools: vec!["Read".to_string(), "Grep".to_string()],
        replaces: Vec::new(),
    }
}

/// The same assistant, declared to stand in for `replaces` on the main agent's behalf — the tools
/// the session's own agent stops being able to call once this assistant is attached.
///
/// A separate fixture rather than a wider `an_assistant`: `tools` and `replaces` are two different
/// vocabularies (what this assistant may call, versus what it takes away from the main agent), and
/// a fixture that set both to the same list could not tell a store that confused them apart.
fn an_assistant_taking_over(provider_id: &str, replaces: &[&str]) -> NewAssistant {
    NewAssistant {
        replaces: replaces.iter().map(|t| t.to_string()).collect(),
        ..an_assistant(provider_id)
    }
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lists_a_created_provider_stamped_with_this_daemon() {
    // Given
    let (_dir, store) = a_store().await;

    // When
    store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let providers = store.list_providers().await.expect("list providers");

    // Then
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].label, "Local Ollama");
    assert_eq!(providers[0].base_url, "http://localhost:11434");
    assert_eq!(providers[0].kind, ProviderKind::Ollama as i32);
    assert_eq!(providers[0].daemon_instance_id, THIS_DAEMON);
}

#[tokio::test]
async fn reports_a_stored_credential_as_a_flag_and_never_as_the_key() {
    // Given a provider created with an api key
    let (_dir, store) = a_store().await;
    store
        .create_provider(a_credentialed_cloud_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // When the provider is read back the way an RPC response would read it
    let providers = store.list_providers().await.expect("list providers");

    // Then the flag is set and the key is nowhere in the returned row
    assert_eq!(providers.len(), 1);
    assert!(providers[0].has_credential);
    // Scanning the whole `{:?}` rather than named fields on purpose: it is a tripwire for a field
    // added to `ProviderEntry` later that carries the credential without anyone noticing.
    let rendered = format!("{:?}", providers[0]);
    assert!(
        !rendered.contains("fw-secret-key"),
        "the api key leaked into the provider row: {rendered}"
    );
}

#[tokio::test]
async fn hands_the_stored_credential_only_to_a_caller_that_asks_for_it_by_provider() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_credentialed_cloud_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // When
    let credential = store
        .credential_for(&provider.provider_id, AN_OPERATOR)
        .await
        .expect("read the credential");

    // Then — the provider client can authenticate; nothing else sees it
    assert_eq!(credential, Some("fw-secret-key".to_string()));
}

#[tokio::test]
async fn reports_no_credential_for_a_keyless_provider() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // When
    let credential = store
        .credential_for(&provider.provider_id, AN_OPERATOR)
        .await
        .expect("read the credential");

    // Then
    assert_eq!(credential, None);
    assert!(!provider.has_credential);
}

#[tokio::test]
async fn refuses_a_second_provider_on_the_same_base_url() {
    // Given an Ollama already configured on this endpoint
    let (_dir, store) = a_store().await;
    store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    // Everything but the endpoint differs, so only the base URL can be what the refusal is about.
    let same_endpoint_different_everything_else = NewProvider {
        kind: ProviderKind::Openai,
        label: "Someone else's name for the same box".to_string(),
        ..a_keyless_ollama_provider()
    };

    // When
    let result = store
        .create_provider(same_endpoint_different_everything_else, AN_OPERATOR)
        .await;

    // Then — the endpoint identifies the provider; a second row for it would double every model
    assert!(
        matches!(result, Err(ModelRegistryError::AlreadyExists(_))),
        "expected AlreadyExists, got {result:?}"
    );
}

#[tokio::test]
async fn mints_the_next_id_for_a_second_provider_of_the_same_kind_on_another_endpoint() {
    // Given an Ollama on this workstation
    let (_dir, store) = a_store().await;
    let first = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the first provider");
    let another_ollama_elsewhere = NewProvider {
        base_url: "http://gpu-box:11434".to_string(),
        label: "GPU box Ollama".to_string(),
        ..a_keyless_ollama_provider()
    };

    // When a second Ollama on a different endpoint is added
    let second = store
        .create_provider(another_ollama_elsewhere, AN_OPERATOR)
        .await
        .expect("create the second provider");

    // Then — both are kept, under readable ids that stay stable for each row's life
    assert_eq!(first.provider_id, "prov-ollama");
    assert_eq!(second.provider_id, "prov-ollama-2");
}

#[tokio::test]
async fn refuses_to_delete_a_provider_an_assistant_still_uses() {
    // Given a provider with an assistant built on one of its models
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    store
        .replace_models(
            &provider.provider_id,
            vec![a_model(&provider.provider_id, "qwen3:32b", &["llm"])],
        )
        .await
        .expect("cache the models");
    store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // When
    let result = store
        .delete_provider(&provider.provider_id, AN_OPERATOR)
        .await;

    // Then — an explicit refusal, not a cascade that silently drops the assistant
    assert!(
        matches!(result, Err(ModelRegistryError::InUse(_))),
        "expected InUse, got {result:?}"
    );
    assert_eq!(
        store.list_providers().await.expect("list providers").len(),
        1
    );
}

#[tokio::test]
async fn removes_a_provider_and_its_cached_models_together() {
    // Given a provider with cached models and no assistant referencing it
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    store
        .replace_models(
            &provider.provider_id,
            vec![a_model(&provider.provider_id, "qwen3:32b", &["llm"])],
        )
        .await
        .expect("cache the models");

    // When
    store
        .delete_provider(&provider.provider_id, AN_OPERATOR)
        .await
        .expect("delete the provider");

    // Then
    assert_eq!(
        store.list_providers().await.expect("list providers").len(),
        0
    );
    assert_eq!(store.list_models().await.expect("list models").len(), 0);
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

#[tokio::test]
async fn replaces_a_providers_cached_models_rather_than_accumulating_them() {
    // Given a provider whose first enumeration found two models
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    store
        .replace_models(
            &provider.provider_id,
            vec![
                a_model(&provider.provider_id, "qwen3:32b", &["llm"]),
                a_model(&provider.provider_id, "nomic-embed-text", &["embedding"]),
            ],
        )
        .await
        .expect("cache the first enumeration");

    // When a later enumeration finds only one (the other was removed on the host)
    store
        .replace_models(
            &provider.provider_id,
            vec![a_model(&provider.provider_id, "qwen3:32b", &["llm"])],
        )
        .await
        .expect("cache the second enumeration");

    // Then the cache reflects the provider, not the union of every enumeration
    let model_ids: Vec<String> = store
        .list_models()
        .await
        .expect("list models")
        .into_iter()
        .map(|m| m.model_id)
        .collect();
    assert_eq!(model_ids, vec!["qwen3:32b".to_string()]);
}

#[tokio::test]
async fn records_an_enumeration_failure_against_the_provider_that_failed() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // When
    store
        .record_enumeration_error(
            &provider.provider_id,
            "connection refused: http://localhost:11434/api/tags",
        )
        .await
        .expect("record the failure");

    // Then
    let providers = store.list_providers().await.expect("list providers");
    assert_eq!(
        providers[0].enumeration_error,
        "connection refused: http://localhost:11434/api/tags"
    );
}

// ---------------------------------------------------------------------------
// Assistants
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lists_a_created_assistant_with_the_tools_it_was_given() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // When
    store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // Then
    let assistants = store.list_assistants().await.expect("list assistants");
    assert_eq!(assistants.len(), 1);
    assert_eq!(assistants[0].name, "repo-reader");
    assert_eq!(assistants[0].model_id, "qwen3:32b");
    assert_eq!(assistants[0].tools, vec!["Read", "Grep"]);
    assert_eq!(assistants[0].daemon_instance_id, THIS_DAEMON);
}

#[tokio::test]
async fn lists_a_created_assistant_with_the_main_agent_tools_it_takes_over() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // When — an assistant whose takeover is deliberately not its own tool set
    store
        .create_assistant(
            an_assistant_taking_over(&provider.provider_id, &["Grep", "Glob"]),
            AN_OPERATOR,
        )
        .await
        .expect("create the assistant");

    // Then the two sets are stored apart: `tools` is what the assistant may call, `replaces` is
    // what the main agent may no longer call while it is attached
    let assistants = store.list_assistants().await.expect("list assistants");
    assert_eq!(assistants[0].tools, vec!["Read", "Grep"]);
    assert_eq!(assistants[0].replaces, vec!["Grep", "Glob"]);
}

#[tokio::test]
async fn lists_an_assistant_created_without_a_takeover_as_taking_over_nothing() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // When
    store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // Then — attaching it takes nothing away from the main agent, which is what an operator who
    // ticked no takeover box asked for
    let assistants = store.list_assistants().await.expect("list assistants");
    assert!(
        assistants[0].replaces.is_empty(),
        "expected no takeover, got {:?}",
        assistants[0].replaces
    );
}

#[tokio::test]
async fn refuses_an_assistant_taking_over_a_tool_outside_the_exec_catalog() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // When — a name no exec tool answers to
    let result = store
        .create_assistant(
            an_assistant_taking_over(&provider.provider_id, &["Telepathy"]),
            AN_OPERATOR,
        )
        .await;

    // Then it is refused rather than dropped: an assistant admitted with the typo silently takes
    // over nothing, and the main agent keeps a tool the operator meant to withdraw
    assert!(
        matches!(&result, Err(ModelRegistryError::UnknownTool(name)) if name == "Telepathy"),
        "expected UnknownTool(Telepathy), got {result:?}"
    );
}

#[tokio::test]
async fn updates_the_main_agent_tools_an_assistant_takes_over() {
    // Given a persisted assistant that takes over one tool
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let assistant = store
        .create_assistant(
            an_assistant_taking_over(&provider.provider_id, &["Grep"]),
            AN_OPERATOR,
        )
        .await
        .expect("create the assistant");

    // When the operator widens the takeover without touching its own tools
    let updated = store
        .update_assistant(
            &assistant.assistant_id,
            "Repo Reader",
            "You read code and answer questions about it.",
            &["Read".to_string(), "Grep".to_string()],
            &["Grep".to_string(), "Glob".to_string()],
            AN_OPERATOR,
        )
        .await
        .expect("update the assistant");

    // Then
    assert_eq!(updated.tools, vec!["Read", "Grep"]);
    assert_eq!(updated.replaces, vec!["Grep", "Glob"]);
}

#[tokio::test]
async fn refuses_to_update_an_assistant_to_take_over_a_tool_outside_the_exec_catalog() {
    // Given a persisted assistant that takes over one tool
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let assistant = store
        .create_assistant(
            an_assistant_taking_over(&provider.provider_id, &["Grep"]),
            AN_OPERATOR,
        )
        .await
        .expect("create the assistant");

    // When
    let result = store
        .update_assistant(
            &assistant.assistant_id,
            "Repo Reader",
            "You read code and answer questions about it.",
            &["Read".to_string()],
            &["Telepathy".to_string()],
            AN_OPERATOR,
        )
        .await;

    // Then the stored takeover is left as it was, rather than replaced by a set the daemon could
    // not enforce
    assert!(
        matches!(&result, Err(ModelRegistryError::UnknownTool(name)) if name == "Telepathy"),
        "expected UnknownTool(Telepathy), got {result:?}"
    );
    let assistants = store.list_assistants().await.expect("list assistants");
    assert_eq!(assistants[0].replaces, vec!["Grep"]);
}

#[tokio::test]
async fn takes_over_tools_on_a_database_created_before_takeovers_existed() {
    // Given a registry database whose `assistant` table predates the takeover column — the shape
    // every already-deployed daemon's file has
    let dir = tempfile::tempdir().expect("a tempdir for the registry db");
    let db_path = dir.path().join("models.db");
    let agents_dir = dir.path().join("agents");
    let store = ModelRegistryStore::open(&db_path, THIS_DAEMON, &agents_dir)
        .await
        .expect("open the registry store");
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    drop(store);
    drop_the_takeover_column(&db_path).await;

    // When the daemon opens that file again and an assistant takes a tool over
    let store = ModelRegistryStore::open(&db_path, THIS_DAEMON, &agents_dir)
        .await
        .expect("reopen the registry store");
    store
        .create_assistant(
            an_assistant_taking_over(&provider.provider_id, &["Grep"]),
            AN_OPERATOR,
        )
        .await
        .expect("create the assistant");

    // Then — an upgrade that skipped the column would fail every assistant write on the daemon
    let assistants = store.list_assistants().await.expect("list assistants");
    assert_eq!(assistants[0].replaces, vec!["Grep"]);
}

/// Put an existing registry file back into its pre-takeover shape, so the migration is exercised
/// against a database this store itself wrote rather than a hand-copied schema that could drift.
async fn drop_the_takeover_column(db_path: &std::path::Path) {
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("open the registry db directly");
    sqlx::query("ALTER TABLE assistant DROP COLUMN replaces")
        .execute(&pool)
        .await
        .expect("the takeover column must be droppable");
    pool.close().await;
}

#[tokio::test]
async fn refuses_a_second_assistant_with_the_same_name() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // When
    let result = store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await;

    // Then — the name is the `--agent` value, so it must stay unique
    assert!(
        matches!(result, Err(ModelRegistryError::AlreadyExists(_))),
        "expected AlreadyExists, got {result:?}"
    );
}

#[tokio::test]
async fn updates_an_assistants_label_prompt_and_tools() {
    // Given a persisted assistant
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let assistant = store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // When
    let updated = store
        .update_assistant(
            &assistant.assistant_id,
            "Repo Reader & Writer",
            "You read code and change it when asked.",
            &["Read".to_string(), "Write".to_string()],
            &[],
            AN_OPERATOR,
        )
        .await
        .expect("update the assistant");

    // Then — the editable parts change and the `--agent` name it is selected by does not
    assert_eq!(updated.label, "Repo Reader & Writer");
    assert_eq!(
        updated.system_prompt,
        "You read code and change it when asked."
    );
    assert_eq!(updated.tools, vec!["Read", "Write"]);
    assert_eq!(updated.name, "repo-reader");
}

#[tokio::test]
async fn refuses_to_update_an_assistant_that_does_not_exist() {
    // Given a registry with no assistants
    let (_dir, store) = a_store().await;

    // When
    let result = store
        .update_assistant(
            "asst-nobody",
            "Ghost",
            "",
            &["Read".to_string()],
            &[],
            AN_OPERATOR,
        )
        .await;

    // Then — an update that matched no row is a failure, never a silent no-op
    assert!(
        matches!(result, Err(ModelRegistryError::NotFound(_))),
        "expected NotFound, got {result:?}"
    );
}

#[tokio::test]
async fn removes_a_deleted_assistant_from_the_listing() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let assistant = store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // When
    store
        .delete_assistant(&assistant.assistant_id, AN_OPERATOR)
        .await
        .expect("delete the assistant");

    // Then — its `--agent` name is free again
    assert_eq!(
        store
            .list_assistants()
            .await
            .expect("list assistants")
            .len(),
        0
    );
}

#[tokio::test]
async fn refuses_to_delete_an_assistant_that_does_not_exist() {
    // Given a registry with no assistants
    let (_dir, store) = a_store().await;

    // When
    let result = store.delete_assistant("asst-nobody", AN_OPERATOR).await;

    // Then
    assert!(
        matches!(result, Err(ModelRegistryError::NotFound(_))),
        "expected NotFound, got {result:?}"
    );
}

#[tokio::test]
async fn refuses_an_assistant_built_on_a_provider_this_daemon_does_not_have() {
    // Given a registry whose only provider is the local Ollama
    let (_dir, store) = a_store().await;
    store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // When
    let result = store
        .create_assistant(an_assistant("prov-nobody"), AN_OPERATOR)
        .await;

    // Then — a row pointing at a provider that does not exist could never be run as an agent
    assert!(
        matches!(result, Err(ModelRegistryError::NotFound(_))),
        "expected NotFound, got {result:?}"
    );
}

#[tokio::test]
async fn refuses_an_assistant_naming_a_tool_outside_the_exec_catalog() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let with_a_bogus_tool = NewAssistant {
        tools: vec!["Read".to_string(), "Teleport".to_string()],
        ..an_assistant(&provider.provider_id)
    };

    // When
    let result = store.create_assistant(with_a_bogus_tool, AN_OPERATOR).await;

    // Then — an unknown tool is rejected, never quietly dropped from the set
    assert!(
        matches!(result, Err(ModelRegistryError::UnknownTool(_))),
        "expected UnknownTool, got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// The credential an assistant's session is started with
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hands_a_registry_assistants_def_the_credential_of_the_provider_it_is_built_on() {
    // Given an assistant on a provider that authenticates
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_credentialed_cloud_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // When the owner starts a session as it
    let def = registry_agent_def_with_credential(&store, "repo-reader", AN_OPERATOR)
        .await
        .expect("resolve the def")
        .expect("the registry defines this agent");

    // Then — without the key every model call 401s, and the session looks merely broken
    assert_eq!(def.api_key.as_deref(), Some("fw-secret-key"));
    assert_eq!(def.base_url, "https://api.fireworks.ai/inference");
}

#[tokio::test]
async fn refuses_another_operator_a_def_built_on_a_provider_that_is_not_theirs() {
    // Given an assistant on the first operator's credentialed provider
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_credentialed_cloud_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // When a colleague starts a session as it
    let result = registry_agent_def_with_credential(&store, "repo-reader", ANOTHER_OPERATOR).await;

    // Then it is refused rather than started against her endpoint with no key at all
    assert!(
        matches!(result, Err(ModelRegistryError::PermissionDenied(_))),
        "expected a permission refusal, got {result:?}"
    );
}

#[tokio::test]
async fn resolves_no_def_for_a_name_this_registry_never_defined() {
    // Given a registry with no assistants
    let (_dir, store) = a_store().await;

    // When
    let def = registry_agent_def_with_credential(&store, "claude", AN_OPERATOR)
        .await
        .expect("resolve the def");

    // Then — a name the registry does not define is not this function's to answer for
    assert!(def.is_none(), "expected no def, got {def:?}");
}

// ---------------------------------------------------------------------------
// Capability labels
// ---------------------------------------------------------------------------

#[test]
fn labels_a_completion_model_that_can_call_tools_as_an_llm_with_tools() {
    // Given the capabilities Ollama's /api/show reports for a chat model
    // When
    let labels = capabilities_to_labels(&["completion".to_string(), "tools".to_string()]);

    // Then
    assert_eq!(labels, vec!["llm".to_string(), "tools".to_string()]);
}

#[test]
fn labels_an_embedding_model_as_embedding_only() {
    // Given / When
    let labels = capabilities_to_labels(&["embedding".to_string()]);

    // Then
    assert_eq!(labels, vec!["embedding".to_string()]);
}

#[test]
fn labels_a_vision_model_as_an_llm_that_sees() {
    // Given / When
    let labels = capabilities_to_labels(&["completion".to_string(), "vision".to_string()]);

    // Then
    assert_eq!(labels, vec!["llm".to_string(), "vision".to_string()]);
}

#[test]
fn labels_a_model_reporting_no_capabilities_as_unknown_rather_than_guessing_llm() {
    // Given a provider that told us nothing about the model
    // When
    let labels = capabilities_to_labels(&[]);

    // Then — the honest answer, not a plausible default
    assert_eq!(labels, vec!["unknown".to_string()]);
}

#[test]
fn labels_a_model_reporting_only_capabilities_it_does_not_recognise_as_unknown() {
    // Given / When
    let labels = capabilities_to_labels(&["teleportation".to_string()]);

    // Then
    assert_eq!(labels, vec!["unknown".to_string()]);
}

#[test]
fn labels_a_listing_the_provider_called_chat_capable_as_an_llm() {
    // Given the per-model flags a Fireworks `/v1/models` entry carries
    // When
    let labels = reported_capabilities_to_labels(Some(true), Some(true), Some(false));

    // Then — the web offers Chat on a positive `llm` label and nothing less
    assert_eq!(labels, vec!["llm".to_string(), "tools".to_string()]);
}

#[test]
fn labels_a_listing_the_provider_said_nothing_about_as_unknown() {
    // Given an OpenAI `/v1/models` entry, which reports no capabilities at all
    // When
    let labels = reported_capabilities_to_labels(None, None, None);

    // Then — silence is not a chat model
    assert_eq!(labels, vec!["unknown".to_string()]);
}

#[test]
fn labels_a_listing_the_provider_called_not_chat_capable_as_unknown() {
    // Given a listing that says what the model is *not* — an embedding model, in practice
    // When
    let labels = reported_capabilities_to_labels(Some(false), Some(false), Some(false));

    // Then — nothing here says what it is, so nothing is claimed
    assert_eq!(labels, vec!["unknown".to_string()]);
}

// ---------------------------------------------------------------------------
// Assistant → SpecializedAgentDef projection (what makes it a selectable --agent)
// ---------------------------------------------------------------------------

/// The provider row as `assistant_to_agent_def` receives it.
fn a_provider_entry() -> ProviderEntry {
    ProviderEntry {
        provider_id: "prov-ollama".to_string(),
        kind: ProviderKind::Ollama as i32,
        label: "Local Ollama".to_string(),
        base_url: "http://localhost:11434".to_string(),
        has_credential: false,
        daemon_instance_id: THIS_DAEMON.to_string(),
        enumeration_error: String::new(),
    }
}

#[tokio::test]
async fn projects_an_assistant_onto_an_agent_def_carrying_its_model_endpoint_and_tools() {
    // Given a persisted assistant and the provider it names
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let assistant = store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // When — projected against the provider row the store actually minted, not a stand-in, so the
    // test says nothing about how provider ids are generated
    let def = assistant_to_agent_def(&assistant, &provider).expect("project the def");

    // Then — everything `create_backend` needs to run it as `--agent repo-reader`
    assert_eq!(def.name, "repo-reader");
    assert_eq!(def.label, Some("Repo Reader".to_string()));
    assert_eq!(def.model, "qwen3:32b");
    assert_eq!(def.base_url, "http://localhost:11434");
    assert_eq!(
        def.system_prompt,
        Some("You read code and answer questions about it.".to_string())
    );
    assert_eq!(def.tools, vec![SubagentTool::Read, SubagentTool::Grep]);
}

#[tokio::test]
async fn projects_an_assistant_onto_an_agent_def_that_withdraws_the_tools_it_takes_over() {
    // Given a persisted assistant that stands in for two of the main agent's tools
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let assistant = store
        .create_assistant(
            an_assistant_taking_over(&provider.provider_id, &["Grep", "Glob"]),
            AN_OPERATOR,
        )
        .await
        .expect("create the assistant");

    // When
    let def = assistant_to_agent_def(&assistant, &provider).expect("project the def");

    // Then the def carries the takeover, which is the only thing `tddy-tools` withdraws a main
    // agent tool from — its own tool set says nothing about what the main agent may still call
    assert_eq!(def.replaces, vec!["Grep".to_string(), "Glob".to_string()]);
    assert_eq!(def.tools, vec![SubagentTool::Read, SubagentTool::Grep]);
}

#[test]
fn refuses_to_project_an_assistant_whose_provider_is_not_the_one_it_names() {
    // Given an assistant built on `prov-ollama` and a row for a different provider
    let assistant = tddy_service::proto::models::AssistantEntry {
        assistant_id: "asst-1".to_string(),
        name: "repo-reader".to_string(),
        label: "Repo Reader".to_string(),
        provider_id: "prov-ollama".to_string(),
        model_id: "qwen3:32b".to_string(),
        system_prompt: String::new(),
        tools: vec!["Read".to_string()],
        replaces: Vec::new(),
        daemon_instance_id: THIS_DAEMON.to_string(),
    };
    let other_provider = ProviderEntry {
        provider_id: "prov-fireworks".to_string(),
        ..a_provider_entry()
    };

    // When
    let result = assistant_to_agent_def(&assistant, &other_provider);

    // Then — a mismatched pairing would silently point the def at the wrong endpoint
    assert!(
        matches!(result, Err(ModelRegistryError::NotFound(_))),
        "expected NotFound, got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// One tool vocabulary (this crate sees both `tddy-discovery` and `tddy-tool-engine`)
// ---------------------------------------------------------------------------

/// The exec-catalog tool names, spelled out. `tddy-discovery` restates this same list in
/// `subagent_tool_exec_catalog_red.rs` because it deliberately does not depend on
/// `tddy-tool-engine`; this crate depends on both, so it is where the two lists are held to each
/// other. An eleventh engine tool fails here rather than quietly leaving the other list short.
const EXEC_CATALOG_NAMES: [&str; 10] = [
    "Read",
    "Write",
    "StrReplace",
    "Delete",
    "Grep",
    "Glob",
    "Shell",
    "Await",
    "ReadLints",
    "SemanticSearch",
];

/// The names of the tools the engine actually dispatches, in catalog order.
fn exec_catalog_names() -> Vec<String> {
    tddy_tool_engine::catalog::tool_catalog()
        .into_iter()
        .map(|t| t.name)
        .collect()
}

#[test]
fn names_the_exec_catalog_exactly_as_tddy_discovery_spells_it() {
    // Given the catalog the daemon actually dispatches
    // When
    let catalog = exec_catalog_names();

    // Then — the vocabulary `tddy-discovery` hard-codes is the engine's, tool for tool
    assert_eq!(catalog, EXEC_CATALOG_NAMES);
}

#[test]
fn resolves_every_tool_the_exec_catalog_advertises_to_a_subagent_tool() {
    // Given the catalog the daemon actually dispatches
    let catalog = exec_catalog_names();

    // When each name is resolved and named again
    let round_tripped: Vec<String> = catalog
        .iter()
        .map(|name| {
            SubagentTool::from_catalog_name(name)
                .unwrap_or_else(|| panic!("exec-catalog tool '{name}' has no SubagentTool variant"))
                .catalog_name()
                .to_string()
        })
        .collect();

    // Then — an assistant can be given any tool the engine can run
    assert_eq!(round_tripped, catalog);
}

// ---------------------------------------------------------------------------
// The database holds plaintext api keys
// ---------------------------------------------------------------------------

/// The permission bits of `path`, or a panic naming the file that should have been there.
#[cfg(unix)]
fn mode_of(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("{} must exist: {e}", path.display()))
        .permissions()
        .mode()
        & 0o777
}

#[cfg(unix)]
#[tokio::test]
async fn creates_the_database_owner_only_because_it_holds_plaintext_api_keys() {
    // Given a daemon started under the usual 0022 umask
    let dir = tempfile::tempdir().expect("a tempdir for the registry db");
    let db_path = dir.path().join("models.db");

    // When the registry is opened and a credentialed provider is stored in it
    let store = ModelRegistryStore::open(&db_path, THIS_DAEMON, &dir.path().join("agents"))
        .await
        .expect("open the registry store");
    store
        .create_provider(a_credentialed_cloud_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // Then no other account on this host can read the key — including out of the write-ahead log,
    // which is where a just-written row actually lives
    assert_eq!(mode_of(&db_path), 0o600, "models.db must be owner-only");
    for suffix in ["-wal", "-shm"] {
        let sibling = std::path::PathBuf::from(format!("{}{suffix}", db_path.display()));
        if sibling.exists() {
            assert_eq!(
                mode_of(&sibling),
                0o600,
                "{} must be owner-only",
                sibling.display()
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn restricts_a_database_an_earlier_daemon_left_world_readable() {
    // Given a registry created before the mode was enforced
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("a tempdir for the registry db");
    let db_path = dir.path().join("models.db");
    {
        let store = ModelRegistryStore::open(&db_path, THIS_DAEMON, &dir.path().join("agents"))
            .await
            .expect("open the registry store");
        store
            .create_provider(a_credentialed_cloud_provider(), AN_OPERATOR)
            .await
            .expect("create the provider");
    }
    std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o644))
        .expect("leave the db world-readable, as an older daemon did");

    // When the daemon restarts
    let _store = ModelRegistryStore::open(&db_path, THIS_DAEMON, &dir.path().join("agents"))
        .await
        .expect("reopen the registry store");

    // Then the keys already in it stop being world-readable, rather than staying exposed until
    // someone notices
    assert_eq!(mode_of(&db_path), 0o600);
}

// ---------------------------------------------------------------------------
// The base URL this daemon will be pointed at
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refuses_a_base_url_that_is_not_an_http_endpoint() {
    // Given
    let (_dir, store) = a_store().await;
    let pointed_at_the_filesystem = NewProvider {
        base_url: "file:///etc/passwd".to_string(),
        ..a_keyless_ollama_provider()
    };

    // When
    let result = store
        .create_provider(pointed_at_the_filesystem, AN_OPERATOR)
        .await;

    // Then — the daemon fetches this url and echoes what comes back, so it must not be anything
    // an authenticated caller feels like naming
    assert!(
        matches!(result, Err(ModelRegistryError::InvalidBaseUrl(_))),
        "expected InvalidBaseUrl, got {result:?}"
    );
}

#[tokio::test]
async fn refuses_a_base_url_carrying_credentials_in_it() {
    // Given
    let (_dir, store) = a_store().await;
    let with_userinfo = NewProvider {
        base_url: "https://someone:hunter2@api.fireworks.ai".to_string(),
        ..a_keyless_ollama_provider()
    };

    // When
    let result = store.create_provider(with_userinfo, AN_OPERATOR).await;

    // Then — userinfo in the url would be echoed back inside every "unreachable" message
    assert!(
        matches!(result, Err(ModelRegistryError::InvalidBaseUrl(_))),
        "expected InvalidBaseUrl, got {result:?}"
    );
    assert_eq!(
        store.list_providers().await.expect("list providers").len(),
        0
    );
}

#[tokio::test]
async fn refuses_a_base_url_that_names_no_host() {
    // Given
    let (_dir, store) = a_store().await;
    let host_forgotten = NewProvider {
        base_url: "localhost:11434".to_string(),
        ..a_keyless_ollama_provider()
    };

    // When
    let result = store.create_provider(host_forgotten, AN_OPERATOR).await;

    // Then
    assert!(
        matches!(result, Err(ModelRegistryError::InvalidBaseUrl(_))),
        "expected InvalidBaseUrl, got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Provider ids are not recycled, and no model row outlives its provider
// ---------------------------------------------------------------------------

#[tokio::test]
async fn never_mints_a_deleted_providers_id_again() {
    // Given an Ollama that was added and then removed
    let (_dir, store) = a_store().await;
    let first = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the first provider");
    store
        .delete_provider(&first.provider_id, AN_OPERATOR)
        .await
        .expect("delete the provider");

    // When another Ollama is added
    let second = store
        .create_provider(
            NewProvider {
                base_url: "http://gpu-box:11434".to_string(),
                ..a_keyless_ollama_provider()
            },
            AN_OPERATOR,
        )
        .await
        .expect("create the second provider");

    // Then it does not inherit the removed provider's id — everything keyed by `prov-ollama` (a
    // model row a refresh is still writing, a log line, a per-row action in a stale browser tab)
    // would otherwise land on this new endpoint
    assert_eq!(first.provider_id, "prov-ollama");
    assert_eq!(second.provider_id, "prov-ollama-2");
}

#[tokio::test]
async fn refuses_to_cache_models_for_a_provider_that_is_no_longer_registered() {
    // Given a provider that was deleted while a refresh of it was in flight
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    store
        .delete_provider(&provider.provider_id, AN_OPERATOR)
        .await
        .expect("delete the provider");

    // When the refresh finally writes what it enumerated
    let result = store
        .replace_models(
            &provider.provider_id,
            vec![a_model(&provider.provider_id, "qwen3:32b", &["llm"])],
        )
        .await;

    // Then it is refused rather than leaving rows behind for an id nothing owns
    assert!(
        matches!(result, Err(ModelRegistryError::NotFound(_))),
        "expected NotFound, got {result:?}"
    );
    assert_eq!(store.list_models().await.expect("list models").len(), 0);
}

#[tokio::test]
async fn records_a_fresh_catalog_and_clears_the_previous_failure_together() {
    // Given a provider whose last enumeration failed
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    store
        .record_enumeration_error(&provider.provider_id, "connection refused")
        .await
        .expect("record the failure");

    // When a later enumeration succeeds
    store
        .record_refresh(
            &provider.provider_id,
            &[a_model(&provider.provider_id, "qwen3:32b", &["llm"])],
        )
        .await
        .expect("record the refresh");

    // Then the screen sees a fresh catalog and no error — never one without the other
    let providers = store.list_providers().await.expect("list providers");
    assert_eq!(providers[0].enumeration_error, "");
    let model_ids: Vec<String> = store
        .list_models()
        .await
        .expect("list models")
        .into_iter()
        .map(|m| m.model_id)
        .collect();
    assert_eq!(model_ids, vec!["qwen3:32b".to_string()]);
}

// ---------------------------------------------------------------------------
// A provider's own words are bounded
// ---------------------------------------------------------------------------

#[test]
fn keeps_a_short_provider_message_verbatim() {
    // Given / When
    let kept = truncate_provider_detail("connection refused: http://localhost:11434/api/tags");

    // Then
    assert_eq!(kept, "connection refused: http://localhost:11434/api/tags");
}

#[test]
fn cuts_a_provider_message_that_would_not_fit_in_a_response() {
    // Given the html error page a gateway answers with
    let error_page = "<html>".to_string() + &"x".repeat(200_000);

    // When
    let cut = truncate_provider_detail(&error_page);

    // Then — short enough that no response is ever built out of it, and honest about the cut
    assert!(
        cut.len() < MAX_PROVIDER_DETAIL_BYTES + 100,
        "kept {} bytes",
        cut.len()
    );
    assert!(cut.starts_with("<html>"));
    assert!(cut.contains("truncated"), "got: {cut}");
}

#[tokio::test]
async fn stores_only_a_bounded_part_of_a_providers_error_page() {
    // Given a provider that answered a refresh with a huge error page
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");

    // When
    store
        .record_enumeration_error(&provider.provider_id, &"x".repeat(200_000))
        .await
        .expect("record the failure");

    // Then — this column is returned by *every* ListProviders, and a payload past ~60 KB is
    // chunk-framed over LiveKit, where a lost frame wedges the call with no error at all
    let providers = store.list_providers().await.expect("list providers");
    assert!(
        providers[0].enumeration_error.len() < MAX_PROVIDER_DETAIL_BYTES + 100,
        "stored {} bytes",
        providers[0].enumeration_error.len()
    );
}

// ---------------------------------------------------------------------------
// Everyone reads, the owner writes
// ---------------------------------------------------------------------------

/// A registry holding one credentialed provider that `AN_OPERATOR` added.
async fn a_store_with_another_operators_provider() -> (tempfile::TempDir, ModelRegistryStore, String)
{
    let (dir, store) = a_store().await;
    let provider = store
        .create_provider(a_credentialed_cloud_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    (dir, store, provider.provider_id)
}

#[tokio::test]
async fn shows_every_operators_providers_because_the_screen_is_a_fleet_overview() {
    // Given two operators, each with their own provider
    let (_dir, store) = a_store().await;
    store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create alice's provider");
    store
        .create_provider(a_credentialed_cloud_provider(), ANOTHER_OPERATOR)
        .await
        .expect("create bob's provider");

    // When either of them looks at the registry
    let providers = store.list_providers().await.expect("list providers");

    // Then they see the whole fleet, not just their own corner of it
    let labels: Vec<String> = providers.into_iter().map(|p| p.label).collect();
    assert_eq!(
        labels,
        vec!["Local Ollama".to_string(), "Fireworks".to_string()]
    );
}

#[tokio::test]
async fn refuses_another_operator_the_key_stored_on_a_provider() {
    // Given a provider alice configured with her own api key
    let (_dir, store, provider_id) = a_store_with_another_operators_provider().await;

    // When bob's chat, refresh or load asks for the credential to talk to it
    let result = store.credential_for(&provider_id, ANOTHER_OPERATOR).await;

    // Then he is refused outright — not handed the key, and not quietly told "no credential" and
    // left to talk to her endpoint unauthenticated
    assert!(
        matches!(result, Err(ModelRegistryError::PermissionDenied(_))),
        "expected PermissionDenied, got {result:?}"
    );
    let rendered = format!("{result:?}");
    assert!(
        !rendered.contains("fw-secret-key"),
        "the refusal leaked the key: {rendered}"
    );
}

#[tokio::test]
async fn hands_the_owner_their_own_providers_key() {
    // Given
    let (_dir, store, provider_id) = a_store_with_another_operators_provider().await;

    // When
    let credential = store
        .credential_for(&provider_id, AN_OPERATOR)
        .await
        .expect("read the credential");

    // Then — ownership gates other operators, not the one who configured it
    assert_eq!(credential, Some("fw-secret-key".to_string()));
}

#[tokio::test]
async fn refuses_to_delete_another_operators_provider() {
    // Given
    let (_dir, store, provider_id) = a_store_with_another_operators_provider().await;

    // When
    let result = store.delete_provider(&provider_id, ANOTHER_OPERATOR).await;

    // Then — the row, and the key in it, survive
    assert!(
        matches!(result, Err(ModelRegistryError::PermissionDenied(_))),
        "expected PermissionDenied, got {result:?}"
    );
    assert_eq!(
        store.list_providers().await.expect("list providers").len(),
        1
    );
}

#[tokio::test]
async fn refuses_to_update_another_operators_assistant() {
    // Given an assistant alice defined
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let assistant = store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // When
    let result = store
        .update_assistant(
            &assistant.assistant_id,
            "Bob's version",
            "Do as I say.",
            &["Shell".to_string()],
            &[],
            ANOTHER_OPERATOR,
        )
        .await;

    // Then — an assistant is a runnable agent; repointing someone else's is not a listing change
    assert!(
        matches!(result, Err(ModelRegistryError::PermissionDenied(_))),
        "expected PermissionDenied, got {result:?}"
    );
    let unchanged = store.list_assistants().await.expect("list assistants");
    assert_eq!(unchanged[0].label, "Repo Reader");
    assert_eq!(unchanged[0].tools, vec!["Read", "Grep"]);
}

#[tokio::test]
async fn refuses_to_delete_another_operators_assistant() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let assistant = store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // When
    let result = store
        .delete_assistant(&assistant.assistant_id, ANOTHER_OPERATOR)
        .await;

    // Then
    assert!(
        matches!(result, Err(ModelRegistryError::PermissionDenied(_))),
        "expected PermissionDenied, got {result:?}"
    );
    assert_eq!(
        store
            .list_assistants()
            .await
            .expect("list assistants")
            .len(),
        1
    );
}

#[tokio::test]
async fn treats_a_row_written_before_ownership_existed_as_unowned() {
    // Given a registry written by a daemon whose schema had no owner column at all
    let dir = tempfile::tempdir().expect("a tempdir for the registry db");
    let db_path = dir.path().join("models.db");
    write_a_pre_ownership_registry(&db_path).await;

    // When this daemon opens it
    let store = ModelRegistryStore::open(&db_path, THIS_DAEMON, &dir.path().join("agents"))
        .await
        .expect("open the registry store");

    // Then the row is still there, and it belongs to nobody: whoever gets there first may use and
    // remove it. Locking it to no one would strand the providers a running daemon already has,
    // and attributing it to whoever restarted the daemon would be a guess about who set it up.
    assert_eq!(
        store.list_providers().await.expect("list providers").len(),
        1
    );
    assert_eq!(
        store
            .credential_for("prov-ollama", ANOTHER_OPERATOR)
            .await
            .expect("read the credential"),
        Some("legacy-key".to_string())
    );
    store
        .delete_provider("prov-ollama", ANOTHER_OPERATOR)
        .await
        .expect("an unowned provider is deletable by any operator");
}

/// Write a registry in the schema that shipped before the owner column existed, with one provider
/// in it. Raw `sqlx` on purpose: the point is a database this build did not create.
async fn write_a_pre_ownership_registry(db_path: &std::path::Path) {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(options)
        .await
        .expect("create the legacy registry db");
    sqlx::query(
        "CREATE TABLE provider (
            provider_id TEXT PRIMARY KEY,
            kind INTEGER NOT NULL,
            label TEXT NOT NULL,
            base_url TEXT NOT NULL UNIQUE,
            credential TEXT,
            credential_ref TEXT,
            enumeration_error TEXT NOT NULL DEFAULT ''
        );",
    )
    .execute(&pool)
    .await
    .expect("create the legacy provider table");
    sqlx::query(
        "INSERT INTO provider (provider_id, kind, label, base_url, credential, credential_ref,
                               enumeration_error)
         VALUES ('prov-ollama', 1, 'Local Ollama', 'http://localhost:11434', 'legacy-key', NULL, '')",
    )
    .execute(&pool)
    .await
    .expect("insert the legacy provider row");
    pool.close().await;
}

// ---------------------------------------------------------------------------
// An assistant needs a model to be an agent at all
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refuses_an_assistant_with_no_model_id() {
    // Given
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let with_no_model = NewAssistant {
        model_id: String::new(),
        ..an_assistant(&provider.provider_id)
    };

    // When
    let result = store.create_assistant(with_no_model, AN_OPERATOR).await;

    // Then — the model id is what reaches the provider as `"model"`; an empty one is an agent def
    // that cannot run, refused now rather than at the first prompt
    assert!(
        matches!(result, Err(ModelRegistryError::InvalidName(_))),
        "expected InvalidName, got {result:?}"
    );
}

#[tokio::test]
async fn refuses_a_system_prompt_past_the_size_a_response_can_carry() {
    // Given a prompt one byte over the ceiling
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let with_an_enormous_prompt = NewAssistant {
        system_prompt: "y".repeat(MAX_SYSTEM_PROMPT_BYTES + 1),
        ..an_assistant(&provider.provider_id)
    };

    // When
    let result = store
        .create_assistant(with_an_enormous_prompt, AN_OPERATOR)
        .await;

    // Then — it rides every `ListAssistants`, every spawn and every provider turn, so it is bounded
    // where it is written rather than wherever it first fails to fit
    assert!(
        matches!(result, Err(ModelRegistryError::InvalidName(_))),
        "expected InvalidName, got {result:?}"
    );
}

#[tokio::test]
async fn refuses_an_edit_that_grows_a_system_prompt_past_that_same_ceiling() {
    // Given an assistant created with a prompt well under the ceiling
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let assistant = store
        .create_assistant(an_assistant(&provider.provider_id), AN_OPERATOR)
        .await
        .expect("create the assistant");

    // When it is edited past it
    let result = store
        .update_assistant(
            &assistant.assistant_id,
            "Repo Reader",
            &"y".repeat(MAX_SYSTEM_PROMPT_BYTES + 1),
            &["Read".to_string()],
            &[],
            AN_OPERATOR,
        )
        .await;

    // Then — a limit only the create path enforces is one an operator gets past by editing
    assert!(
        matches!(result, Err(ModelRegistryError::InvalidName(_))),
        "expected InvalidName, got {result:?}"
    );
}

#[tokio::test]
async fn accepts_a_system_prompt_that_exactly_fills_the_ceiling() {
    // Given a prompt of exactly the maximum size
    let (_dir, store) = a_store().await;
    let provider = store
        .create_provider(a_keyless_ollama_provider(), AN_OPERATOR)
        .await
        .expect("create the provider");
    let at_the_ceiling = NewAssistant {
        system_prompt: "y".repeat(MAX_SYSTEM_PROMPT_BYTES),
        ..an_assistant(&provider.provider_id)
    };

    // When
    let assistant = store
        .create_assistant(at_the_ceiling, AN_OPERATOR)
        .await
        .expect("a prompt at the ceiling is allowed");

    // Then
    assert_eq!(assistant.system_prompt.len(), MAX_SYSTEM_PROMPT_BYTES);
}
