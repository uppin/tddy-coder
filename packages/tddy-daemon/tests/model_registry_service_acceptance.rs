//! `ModelRegistryService` behavior: auth gating, honest error surfacing (no cached-catalog
//! fallback), residency operations refused where residency has no meaning, and an assistant
//! becoming a selectable `--agent` the moment it is created.
//!
//! Provider I/O is behind a `ProviderClient` fake so these tests exercise the service's own rules,
//! not HTTP. The Ollama wire contract is pinned separately in
//! `ollama_provider_client_integration.rs`.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC7, AC9, AC11).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_daemon::model_registry::{
    ModelRegistryError, ModelRegistryServiceImpl, ModelRegistryStore, NewProvider, ProviderClient,
    ProviderClientFactory,
};
use tddy_rpc::{Code, Request};
use tddy_service::proto::models::{
    CreateAssistantRequest, CreateProviderRequest, ListAssignableToolsRequest, ListModelsRequest,
    ListProvidersRequest, LoadModelRequest, ModelEntry, ModelLoadState,
    ModelRegistryService as ModelRegistryServiceTrait, ProviderEntry, ProviderKind,
    RefreshProviderModelsRequest, UnloadModelRequest,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_DAEMON: &str = "workstation-1";
const VALID_TOKEN: &str = "valid-token";

/// The operator `VALID_TOKEN` resolves to — and so the owner of everything these tests create.
const THE_OPERATOR: &str = "testuser";

/// A second operator on the same daemon, and the token they present.
const ANOTHER_OPERATOR: &str = "bob";
const ANOTHER_OPERATORS_TOKEN: &str = "bobs-token";

type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// What the fake provider client answers with, per provider id.
#[derive(Clone, Default)]
struct FakeProviderBehavior {
    models: Vec<ModelEntry>,
    /// When set, every enumeration fails with this message.
    enumeration_error: Option<String>,
    /// When true, load/unload are refused the way a cloud provider refuses them.
    residency_unsupported: bool,
}

/// A provider row to remove *while* an enumeration is running — the race the refresh path's error
/// handling has to survive. Empty unless a test arms it.
type VanishDuringEnumeration = Arc<Mutex<Option<(Arc<ModelRegistryStore>, String)>>>;

#[derive(Clone)]
struct FakeProviderClient {
    behavior: FakeProviderBehavior,
    loaded: Arc<Mutex<Vec<String>>>,
    unloaded: Arc<Mutex<Vec<String>>>,
    vanish: VanishDuringEnumeration,
}

#[async_trait]
impl ProviderClient for FakeProviderClient {
    async fn list_models(&self) -> Result<Vec<ModelEntry>, ModelRegistryError> {
        let armed = self.vanish.lock().unwrap().take();
        if let Some((store, provider_id)) = armed {
            store
                .delete_provider(&provider_id, THE_OPERATOR)
                .await
                .expect("the provider must be deletable mid-enumeration");
        }
        match &self.behavior.enumeration_error {
            Some(message) => Err(ModelRegistryError::Provider(message.clone())),
            None => Ok(self.behavior.models.clone()),
        }
    }

    async fn load_state(&self, _model_id: &str) -> Result<ModelLoadState, ModelRegistryError> {
        Ok(if self.behavior.residency_unsupported {
            ModelLoadState::Unsupported
        } else {
            ModelLoadState::NotLoaded
        })
    }

    async fn load(&self, model_id: &str) -> Result<(), ModelRegistryError> {
        if self.behavior.residency_unsupported {
            return Err(ModelRegistryError::UnsupportedOperation(
                "residency is not supported for this provider kind".to_string(),
            ));
        }
        self.loaded.lock().unwrap().push(model_id.to_string());
        Ok(())
    }

    async fn unload(&self, model_id: &str) -> Result<(), ModelRegistryError> {
        if self.behavior.residency_unsupported {
            return Err(ModelRegistryError::UnsupportedOperation(
                "residency is not supported for this provider kind".to_string(),
            ));
        }
        self.unloaded.lock().unwrap().push(model_id.to_string());
        Ok(())
    }
}

/// Hands the same fake to every provider — these tests never exercise more than one at a time.
struct FakeProviderClients {
    client: FakeProviderClient,
}

impl ProviderClientFactory for FakeProviderClients {
    fn client_for(
        &self,
        _provider: &ProviderEntry,
        _credential: Option<String>,
    ) -> Result<Arc<dyn ProviderClient>, ModelRegistryError> {
        Ok(Arc::new(self.client.clone()))
    }
}

struct Harness {
    _dir: tempfile::TempDir,
    service: ModelRegistryServiceImpl,
    store: Arc<ModelRegistryStore>,
    loaded: Arc<Mutex<Vec<String>>>,
    unloaded: Arc<Mutex<Vec<String>>>,
    vanish: VanishDuringEnumeration,
}

impl Harness {
    /// Arm the provider row to be deleted the moment the next enumeration starts.
    fn given_the_provider_vanishes_mid_enumeration(&self, provider_id: &str) {
        *self.vanish.lock().unwrap() = Some((Arc::clone(&self.store), provider_id.to_string()));
    }
}

async fn a_service_with(behavior: FakeProviderBehavior) -> Harness {
    let dir = tempfile::tempdir().expect("a tempdir for the registry db");
    let store = Arc::new(
        ModelRegistryStore::open(&dir.path().join("models.db"), THIS_DAEMON)
            .await
            .expect("open the registry store"),
    );
    let loaded = Arc::new(Mutex::new(Vec::new()));
    let unloaded = Arc::new(Mutex::new(Vec::new()));
    let vanish: VanishDuringEnumeration = Arc::new(Mutex::new(None));
    let client = FakeProviderClient {
        behavior,
        loaded: Arc::clone(&loaded),
        unloaded: Arc::clone(&unloaded),
        vanish: Arc::clone(&vanish),
    };
    // Two operators share this daemon, as they do in system mode: each token resolves to the
    // account whose rows it may write.
    let user_resolver: UserResolver = Arc::new(|token| match token {
        VALID_TOKEN => Some(THE_OPERATOR.to_string()),
        ANOTHER_OPERATORS_TOKEN => Some(ANOTHER_OPERATOR.to_string()),
        _ => None,
    });
    let service = ModelRegistryServiceImpl::new(
        Arc::clone(&store),
        Arc::new(FakeProviderClients { client }),
        user_resolver,
    );
    Harness {
        _dir: dir,
        service,
        store,
        loaded,
        unloaded,
        vanish,
    }
}

fn an_ollama_provider() -> NewProvider {
    NewProvider {
        kind: ProviderKind::Ollama,
        label: "Local Ollama".to_string(),
        base_url: "http://localhost:11434".to_string(),
        api_key: None,
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

// ---------------------------------------------------------------------------
// Auth gating
// ---------------------------------------------------------------------------

/// The token of a caller this daemon has never issued a session to.
const AN_UNRECOGNISED_TOKEN: &str = "who-is-this";

#[tokio::test]
async fn refuses_to_list_providers_for_an_unrecognised_session_token() {
    // Given
    let harness = a_service_with(FakeProviderBehavior::default()).await;

    // When
    let result = harness
        .service
        .list_providers(Request::new(ListProvidersRequest {
            session_token: AN_UNRECOGNISED_TOKEN.to_string(),
        }))
        .await;

    // Then
    assert_eq!(
        result.expect_err("expected a rejection").code(),
        Code::Unauthenticated
    );
}

// Every RPC authenticates for itself, so the gate is pinned per RPC rather than once for the
// service: the mutating ones below are the ones an unauthenticated caller could do damage with.

#[tokio::test]
async fn refuses_to_create_a_provider_for_an_unrecognised_session_token() {
    // Given
    let harness = a_service_with(FakeProviderBehavior::default()).await;

    // When
    let result = harness
        .service
        .create_provider(Request::new(CreateProviderRequest {
            session_token: AN_UNRECOGNISED_TOKEN.to_string(),
            kind: ProviderKind::Ollama as i32,
            label: "Local Ollama".to_string(),
            base_url: "http://localhost:11434".to_string(),
            api_key: String::new(),
        }))
        .await;

    // Then — rejected, and nothing was written on the way to the rejection
    assert_eq!(
        result.expect_err("expected a rejection").code(),
        Code::Unauthenticated
    );
    assert_eq!(
        harness
            .store
            .list_providers()
            .await
            .expect("list providers")
            .len(),
        0
    );
}

#[tokio::test]
async fn refuses_to_create_an_assistant_for_an_unrecognised_session_token() {
    // Given a daemon with a provider an assistant could be built on
    let harness = a_service_with(FakeProviderBehavior::default()).await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");

    // When
    let result = harness
        .service
        .create_assistant(Request::new(CreateAssistantRequest {
            session_token: AN_UNRECOGNISED_TOKEN.to_string(),
            name: "repo-reader".to_string(),
            label: "Repo Reader".to_string(),
            provider_id: provider.provider_id.clone(),
            model_id: "qwen3:32b".to_string(),
            system_prompt: "You read code.".to_string(),
            tools: vec!["Read".to_string()],
        }))
        .await;

    // Then — rejected, and no new `--agent` name was minted
    assert_eq!(
        result.expect_err("expected a rejection").code(),
        Code::Unauthenticated
    );
    assert_eq!(
        harness
            .store
            .list_assistants()
            .await
            .expect("list assistants")
            .len(),
        0
    );
}

#[tokio::test]
async fn refuses_to_load_a_model_for_an_unrecognised_session_token() {
    // Given
    let harness = a_service_with(FakeProviderBehavior {
        models: vec![a_model("prov-1", "qwen3:32b", &["llm"])],
        ..Default::default()
    })
    .await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");

    // When
    let result = harness
        .service
        .load_model(Request::new(LoadModelRequest {
            session_token: AN_UNRECOGNISED_TOKEN.to_string(),
            provider_id: provider.provider_id.clone(),
            model_id: "qwen3:32b".to_string(),
        }))
        .await;

    // Then — rejected, and the provider was never reached
    assert_eq!(
        result.expect_err("expected a rejection").code(),
        Code::Unauthenticated
    );
    assert!(harness.loaded.lock().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Enumeration: no fallback
// ---------------------------------------------------------------------------

#[tokio::test]
async fn caches_the_models_a_refresh_enumerated() {
    // Given a provider whose endpoint offers two models
    let harness = a_service_with(FakeProviderBehavior {
        models: vec![
            a_model("prov-1", "qwen3:32b", &["llm", "tools"]),
            a_model("prov-1", "nomic-embed-text", &["embedding"]),
        ],
        ..Default::default()
    })
    .await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");

    // When
    harness
        .service
        .refresh_provider_models(Request::new(RefreshProviderModelsRequest {
            session_token: VALID_TOKEN.to_string(),
            provider_id: provider.provider_id.clone(),
        }))
        .await
        .expect("refresh the catalog");

    // Then
    let listed = harness
        .service
        .list_models(Request::new(ListModelsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("list models")
        .into_inner();
    let model_ids: Vec<String> = listed.models.into_iter().map(|m| m.model_id).collect();
    assert_eq!(
        model_ids,
        vec!["qwen3:32b".to_string(), "nomic-embed-text".to_string()]
    );
}

#[tokio::test]
async fn fails_a_refresh_that_could_not_reach_the_provider_instead_of_returning_a_catalog() {
    // Given a provider whose endpoint is down
    let harness = a_service_with(FakeProviderBehavior {
        enumeration_error: Some("connection refused: http://localhost:11434/api/tags".to_string()),
        ..Default::default()
    })
    .await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");

    // When
    let result = harness
        .service
        .refresh_provider_models(Request::new(RefreshProviderModelsRequest {
            session_token: VALID_TOKEN.to_string(),
            provider_id: provider.provider_id.clone(),
        }))
        .await;

    // Then — the failure reaches the caller verbatim
    let status = result.expect_err("expected the refresh to fail");
    assert_eq!(status.code(), Code::Unavailable);
    assert!(
        status.message().contains("connection refused"),
        "expected the provider's own message, got: {}",
        status.message()
    );
}

#[tokio::test]
async fn records_a_failed_refresh_against_the_provider_so_the_screen_can_show_it() {
    // Given
    let harness = a_service_with(FakeProviderBehavior {
        enumeration_error: Some("connection refused: http://localhost:11434/api/tags".to_string()),
        ..Default::default()
    })
    .await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");

    // When
    harness
        .service
        .refresh_provider_models(Request::new(RefreshProviderModelsRequest {
            session_token: VALID_TOKEN.to_string(),
            provider_id: provider.provider_id.clone(),
        }))
        .await
        .expect_err("expected the refresh to fail");

    // Then
    let providers = harness
        .service
        .list_providers(Request::new(ListProvidersRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("list providers")
        .into_inner()
        .providers;
    assert_eq!(
        providers[0].enumeration_error,
        "connection refused: http://localhost:11434/api/tags"
    );
}

// ---------------------------------------------------------------------------
// Residency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refuses_to_unload_a_model_whose_provider_has_no_notion_of_residency() {
    // Given a cloud provider whose models are never resident
    let harness = a_service_with(FakeProviderBehavior {
        models: vec![a_model("prov-1", "kimi-k2", &["llm"])],
        residency_unsupported: true,
        ..Default::default()
    })
    .await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");

    // When
    let result = harness
        .service
        .unload_model(Request::new(UnloadModelRequest {
            session_token: VALID_TOKEN.to_string(),
            provider_id: provider.provider_id.clone(),
            model_id: "kimi-k2".to_string(),
        }))
        .await;

    // Then
    let status = result.expect_err("expected the unload to be refused");
    assert_eq!(status.code(), Code::FailedPrecondition);
    assert!(harness.unloaded.lock().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Assignable tools
// ---------------------------------------------------------------------------

#[tokio::test]
async fn advertises_the_exec_catalog_as_the_tools_an_assistant_may_be_given() {
    // Given
    let harness = a_service_with(FakeProviderBehavior::default()).await;

    // When
    let tools = harness
        .service
        .list_assignable_tools(Request::new(ListAssignableToolsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("list assignable tools")
        .into_inner()
        .tools;

    // Then — the daemon's catalog, verbatim; the web adds nothing of its own. Spelled out rather
    // than compared against `tool_catalog()`, which is the very thing this RPC returns — that
    // comparison would hold even if the catalog were empty.
    let names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
    assert_eq!(
        names,
        [
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
        ]
    );
}

#[tokio::test]
async fn marks_the_worktree_changing_tools_as_mutating_in_the_assignable_catalog() {
    // Given
    let harness = a_service_with(FakeProviderBehavior::default()).await;

    // When
    let tools = harness
        .service
        .list_assignable_tools(Request::new(ListAssignableToolsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("list assignable tools")
        .into_inner()
        .tools;

    // Then
    let mutating: Vec<String> = tools
        .into_iter()
        .filter(|t| t.is_mutating)
        .map(|t| t.name)
        .collect();
    assert_eq!(mutating, vec!["Write", "StrReplace", "Delete", "Shell"]);
}

// ---------------------------------------------------------------------------
// An assistant is a selectable agent (AC9)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refuses_an_assistant_named_after_a_builtin_agent() {
    // Given
    let harness = a_service_with(FakeProviderBehavior {
        models: vec![a_model("prov-1", "qwen3:32b", &["llm"])],
        ..Default::default()
    })
    .await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");

    // When — `fastcontext` is a builtin def, so the name is already taken
    let result = harness
        .service
        .create_assistant(Request::new(CreateAssistantRequest {
            session_token: VALID_TOKEN.to_string(),
            name: "fastcontext".to_string(),
            label: "Fast Context".to_string(),
            provider_id: provider.provider_id.clone(),
            model_id: "qwen3:32b".to_string(),
            system_prompt: String::new(),
            tools: vec!["Read".to_string()],
        }))
        .await;

    // Then
    assert_eq!(
        result.expect_err("expected the name to be refused").code(),
        Code::AlreadyExists
    );
}

#[tokio::test]
async fn lists_a_created_assistant_among_the_daemons_selectable_agents() {
    // Given a daemon whose config allowlists the usual coding backends
    let harness = a_service_with(FakeProviderBehavior {
        models: vec![a_model("prov-1", "qwen3:32b", &["llm"])],
        ..Default::default()
    })
    .await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");
    let config = a_config_allowing(&["claude", "cursor"]);

    // When an assistant is created
    harness
        .service
        .create_assistant(Request::new(CreateAssistantRequest {
            session_token: VALID_TOKEN.to_string(),
            name: "repo-reader".to_string(),
            label: "Repo Reader".to_string(),
            provider_id: provider.provider_id.clone(),
            model_id: "qwen3:32b".to_string(),
            system_prompt: "You read code.".to_string(),
            tools: vec!["Read".to_string(), "Grep".to_string()],
        }))
        .await
        .expect("create the assistant");

    // Then it joins the allowlist rows `ListAgents` renders, under its own name
    let assistants = harness
        .store
        .list_assistants()
        .await
        .expect("list assistants");
    let rows = tddy_daemon::agent_list_mapping::agent_allowlist_rows(&config, &assistants);
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    assert_eq!(
        ids,
        vec![
            "claude".to_string(),
            "cursor".to_string(),
            "repo-reader".to_string()
        ]
    );
    let labels: Vec<String> = rows.iter().map(|r| r.display_label.clone()).collect();
    assert_eq!(labels[2], "Repo Reader");
}

/// A daemon config whose `allowed_agents` are exactly `ids`.
fn a_config_allowing(ids: &[&str]) -> tddy_daemon::config::DaemonConfig {
    let agents = ids
        .iter()
        .map(|id| format!("  - id: \"{id}\"\n"))
        .collect::<String>();
    let yaml = format!(
        "users:\n  - github_user: \"gh1\"\n    os_user: \"testuser\"\nallowed_agents:\n{agents}"
    );
    let dir = tempfile::tempdir().expect("a tempdir for the daemon config");
    let path: PathBuf = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).expect("write the daemon config");
    let config = tddy_daemon::config::DaemonConfig::load(&path).expect("load the daemon config");
    // The tempdir has done its job once the YAML is parsed.
    drop(dir);
    config
}

// ---------------------------------------------------------------------------
// Owner-only writes (everyone reads)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refuses_to_delete_a_provider_another_operator_added() {
    // Given a provider the first operator configured
    let harness = a_service_with(FakeProviderBehavior::default()).await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");

    // When a colleague on the same daemon deletes it
    let result = harness
        .service
        .delete_provider(Request::new(
            tddy_service::proto::models::DeleteProviderRequest {
                session_token: ANOTHER_OPERATORS_TOKEN.to_string(),
                provider_id: provider.provider_id.clone(),
            },
        ))
        .await;

    // Then — the registry holds every operator's api keys, so a write is the owner's alone
    assert_eq!(
        result.expect_err("expected a refusal").code(),
        Code::PermissionDenied
    );
    assert_eq!(
        harness
            .store
            .list_providers()
            .await
            .expect("list providers")
            .len(),
        1
    );
}

#[tokio::test]
async fn refuses_to_refresh_a_provider_another_operator_added() {
    // Given a provider the first operator configured
    let harness = a_service_with(FakeProviderBehavior {
        models: vec![a_model("prov-1", "qwen3:32b", &["llm"])],
        ..Default::default()
    })
    .await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");

    // When a colleague refreshes it
    let result = harness
        .service
        .refresh_provider_models(Request::new(RefreshProviderModelsRequest {
            session_token: ANOTHER_OPERATORS_TOKEN.to_string(),
            provider_id: provider.provider_id.clone(),
        }))
        .await;

    // Then — refusing beats running against someone else's endpoint without their credential and
    // overwriting their catalog with whatever came back
    assert_eq!(
        result.expect_err("expected a refusal").code(),
        Code::PermissionDenied
    );
}

#[tokio::test]
async fn lists_every_operators_providers_to_whoever_asks() {
    // Given one provider per operator
    let harness = a_service_with(FakeProviderBehavior::default()).await;
    harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the first operator's provider");
    harness
        .store
        .create_provider(
            NewProvider {
                label: "Bob's Ollama".to_string(),
                base_url: "http://gpu-box:11434".to_string(),
                ..an_ollama_provider()
            },
            ANOTHER_OPERATOR,
        )
        .await
        .expect("create the second operator's provider");

    // When either of them opens the screen
    let providers = harness
        .service
        .list_providers(Request::new(ListProvidersRequest {
            session_token: ANOTHER_OPERATORS_TOKEN.to_string(),
        }))
        .await
        .expect("list providers")
        .into_inner()
        .providers;

    // Then the overview is the fleet's, not one account's
    let labels: Vec<String> = providers.into_iter().map(|p| p.label).collect();
    assert_eq!(
        labels,
        vec!["Local Ollama".to_string(), "Bob's Ollama".to_string()]
    );
}

// ---------------------------------------------------------------------------
// A refresh reports what actually failed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reports_the_providers_failure_even_when_recording_it_could_not_be_done() {
    // Given a provider that is deleted while its enumeration is in flight, and fails anyway
    let harness = a_service_with(FakeProviderBehavior {
        enumeration_error: Some("connection refused: http://localhost:11434/api/tags".to_string()),
        ..Default::default()
    })
    .await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");
    harness.given_the_provider_vanishes_mid_enumeration(&provider.provider_id);

    // When
    let result = harness
        .service
        .refresh_provider_models(Request::new(RefreshProviderModelsRequest {
            session_token: VALID_TOKEN.to_string(),
            provider_id: provider.provider_id.clone(),
        }))
        .await;

    // Then the caller is told what their provider did — recording it for the screen is a courtesy,
    // and failing at that must not replace the only fact worth knowing
    let status = result.expect_err("expected the refresh to fail");
    assert_eq!(status.code(), Code::Unavailable);
    assert!(
        status.message().contains("connection refused"),
        "expected the provider's own message, got: {}",
        status.message()
    );
}

// ---------------------------------------------------------------------------
// A model the catalog has never seen
// ---------------------------------------------------------------------------

#[tokio::test]
async fn labels_a_model_outside_the_cached_catalog_as_unknown_rather_than_unlabelled() {
    // Given a provider whose catalog has never been refreshed
    let harness = a_service_with(FakeProviderBehavior::default()).await;
    let provider = harness
        .store
        .create_provider(an_ollama_provider(), THE_OPERATOR)
        .await
        .expect("create the provider");

    // When a model nobody has enumerated is loaded by name
    let model = harness
        .service
        .load_model(Request::new(LoadModelRequest {
            session_token: VALID_TOKEN.to_string(),
            provider_id: provider.provider_id.clone(),
            model_id: "qwen3:32b".to_string(),
        }))
        .await
        .expect("load the model")
        .into_inner()
        .model
        .expect("the response must carry the model");

    // Then it is labelled the way an undeterminable model is labelled everywhere else. An empty
    // list is not "we could not tell" — it reads as "this model has no capabilities at all", which
    // is what the screen would then render.
    assert_eq!(model.labels, vec!["unknown".to_string()]);
}

// ---------------------------------------------------------------------------
// Which client a provider row resolves to
// ---------------------------------------------------------------------------

/// A stored provider row of `kind`, as the production factory receives it.
fn a_provider_row_of_kind(kind: i32) -> ProviderEntry {
    ProviderEntry {
        provider_id: "prov-1".to_string(),
        kind,
        label: "Something".to_string(),
        base_url: "https://api.example.com".to_string(),
        has_credential: true,
        daemon_instance_id: THIS_DAEMON.to_string(),
        enumeration_error: String::new(),
    }
}

#[tokio::test]
async fn refuses_to_resolve_a_client_for_a_provider_row_with_no_kind() {
    // Given a row whose kind never got set
    let unspecified = a_provider_row_of_kind(ProviderKind::Unspecified as i32);

    // When
    let result = tddy_daemon::model_registry::DefaultProviderClients
        .client_for(&unspecified, Some("a-real-api-key".to_string()));

    // Then — resolving it to "probably OpenAI-compatible" would send this key to an endpoint
    // nobody decided it belonged to
    assert!(
        matches!(result, Err(ModelRegistryError::UnsupportedOperation(_))),
        "expected a refusal, got a client"
    );
}

#[tokio::test]
async fn refuses_to_resolve_a_client_for_a_provider_kind_this_build_does_not_know() {
    // Given a row written by a newer daemon into the same database
    let from_the_future = a_provider_row_of_kind(97);

    // When
    let result = tddy_daemon::model_registry::DefaultProviderClients
        .client_for(&from_the_future, Some("a-real-api-key".to_string()));

    // Then
    assert!(
        matches!(result, Err(ModelRegistryError::UnsupportedOperation(_))),
        "expected a refusal, got a client"
    );
}

#[tokio::test]
async fn resolves_every_kind_it_does_know_to_a_client() {
    // Given / When / Then — the arms that must stay reachable, so the refusals above cannot be
    // satisfied by refusing everything
    for kind in [
        ProviderKind::Ollama,
        ProviderKind::Openai,
        ProviderKind::Fireworks,
        ProviderKind::Anthropic,
    ] {
        let row = a_provider_row_of_kind(kind as i32);
        assert!(
            tddy_daemon::model_registry::DefaultProviderClients
                .client_for(&row, Some("a-real-api-key".to_string()))
                .is_ok(),
            "{kind:?} must resolve to a client"
        );
    }
}

// ---------------------------------------------------------------------------
// What a storage failure tells the caller
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tells_the_caller_nothing_about_the_daemons_own_filesystem_when_storage_fails() {
    // Given a storage failure carrying the detail sqlx puts in one
    let failure = ModelRegistryError::Storage(sqlx::Error::Io(std::io::Error::other(
        "/srv/tddy/data/models.db: disk quota exceeded",
    )));

    // When it is turned into what the caller is told
    let status = tddy_rpc::Status::from(failure);

    // Then the daemon's paths stay on the daemon; the detail is logged instead
    assert_eq!(status.code(), Code::Internal);
    assert!(
        !status.message().contains("/srv/tddy/data"),
        "the status leaked a host path: {}",
        status.message()
    );
    assert!(
        !status.message().contains("quota"),
        "the status leaked the storage detail: {}",
        status.message()
    );
}
