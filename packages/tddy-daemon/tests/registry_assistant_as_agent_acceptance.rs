//! An assistant created in the Models & Agents screen is not merely *listed* as an agent — it is
//! usable as one: this daemon's registry is a specialized-agent-def source alongside the builtins
//! and `<tddyhome>/agents/*.yaml`, and `StartSession` accepts its name as `--agent`.
//!
//! These are the two seams `ListAgents` alone cannot prove. `list_agents_allowlist_acceptance.rs`
//! pins the listing half; this suite pins resolution and session start.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC9).

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::model_registry::{ModelRegistryStore, NewAssistant, NewProvider};
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, StartSessionRequest,
};
use tddy_service::proto::models::ProviderKind;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_DAEMON: &str = "workstation-1";
const VALID_TOKEN: &str = "valid-token";
const OLLAMA_URL: &str = "http://127.0.0.1:11434";

/// The key stored on the provider these assistants are built on.
const THE_PROVIDERS_KEY: &str = "fw-live-secret";

/// The operator `VALID_TOKEN` resolves to — the owner of the registry rows these tests create.
const THE_OPERATOR: &str = "testuser";

/// The daemon config every test here runs against: one mapped user and a non-empty
/// `allowed_agents`, which is the configuration under which a registry assistant used to be
/// rejected outright.
const DAEMON_YAML: &str = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
allowed_agents:
  - id: claude
    label: Claude
"#;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The daemon under test, holding a registry with one Ollama provider.
struct Harness {
    _dir: tempfile::TempDir,
    service: ConnectionServiceImpl,
    store: Arc<ModelRegistryStore>,
}

impl Harness {
    /// Define an assistant on `qwen3:32b` with the given name and tool set.
    async fn given_an_assistant_named(&self, name: &str, tools: &[&str]) {
        self.store
            .create_assistant(
                NewAssistant {
                    name: name.to_string(),
                    label: "Repo explorer".to_string(),
                    provider_id: "prov-ollama".to_string(),
                    model_id: "qwen3:32b".to_string(),
                    system_prompt: "You explore repositories.".to_string(),
                    tools: tools.iter().map(|t| t.to_string()).collect(),
                },
                THE_OPERATOR,
            )
            .await
            .expect("the assistant must be created");
    }

    /// Start a session as `agent` against a project that does not exist, so the outcome reports
    /// how far the request got: rejected at the agent gate, or past it.
    async fn when_starting_a_session_as(&self, agent: &str) -> tddy_rpc::Status {
        self.service
            .start_session(Request::new(StartSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                tool_path: "/bin/true".to_string(),
                project_id: "no-such-project".to_string(),
                agent: agent.to_string(),
                ..Default::default()
            }))
            .await
            .expect_err("no project exists, so no session can actually start")
    }
}

async fn a_daemon_with_a_registry() -> Harness {
    let dir = tempfile::tempdir().expect("a tempdir for the daemon's data");
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, DAEMON_YAML).expect("write the daemon config");
    let config = DaemonConfig::load(&config_path).expect("the daemon config must parse");

    let store = Arc::new(
        ModelRegistryStore::open(&dir.path().join("models.db"), THIS_DAEMON)
            .await
            .expect("open the registry store")
            .reserving_agent_ids(config.allowed_agents().iter().map(|a| a.id.clone())),
    );
    store
        .create_provider(
            NewProvider {
                kind: ProviderKind::Ollama,
                label: "Workstation Ollama".to_string(),
                base_url: OLLAMA_URL.to_string(),
                api_key: Some(THE_PROVIDERS_KEY.to_string()),
            },
            THE_OPERATOR,
        )
        .await
        .expect("the provider must be created");

    let tddy_data_dir = dir.path().to_path_buf();
    let sessions_base = tddy_data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    let service = ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
    .with_model_registry(Arc::clone(&store));

    Harness {
        _dir: dir,
        service,
        store,
    }
}

/// The resolved def for `name`, or a panic naming what the daemon could resolve instead.
fn def_named(defs: &[SpecializedAgentDef], name: &str) -> SpecializedAgentDef {
    defs.iter()
        .find(|d| d.name == name)
        .unwrap_or_else(|| {
            panic!(
                "'{name}' is not a resolvable agent def; the daemon resolved: {:?}",
                defs.iter().map(|d| &d.name).collect::<Vec<_>>()
            )
        })
        .clone()
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_registry_assistant_resolves_as_an_agent_def_carrying_its_model_endpoint_and_tools() {
    // Given
    let harness = a_daemon_with_a_registry().await;
    harness
        .given_an_assistant_named("repo-explorer", &["Read", "Grep"])
        .await;

    // When
    let defs = harness
        .service
        .resolvable_agent_defs()
        .await
        .expect("the daemon must resolve its agent defs");

    // Then
    let explorer = def_named(&defs, "repo-explorer");
    assert_eq!(explorer.model, "qwen3:32b");
    assert_eq!(explorer.base_url, OLLAMA_URL);
    assert_eq!(explorer.tools, vec![SubagentTool::Read, SubagentTool::Grep]);
    assert_eq!(
        explorer.system_prompt.as_deref(),
        Some("You explore repositories.")
    );
}

#[tokio::test]
async fn the_def_a_session_is_started_from_carries_its_providers_credential() {
    // Given an assistant on a provider that authenticates
    let harness = a_daemon_with_a_registry().await;
    harness
        .given_an_assistant_named("repo-explorer", &["Read"])
        .await;

    // When the daemon resolves what to start a session as
    let def = harness
        .service
        .agent_def_for_spawn("repo-explorer", THE_OPERATOR)
        .await
        .expect("the daemon must resolve the def")
        .expect("the registry defines this agent");

    // Then — without it the session starts "successfully" and 401s on every model call
    assert_eq!(def.api_key.as_deref(), Some(THE_PROVIDERS_KEY));
}

#[tokio::test]
async fn the_defs_listed_to_every_operator_carry_no_credential_at_all() {
    // Given the same assistant
    let harness = a_daemon_with_a_registry().await;
    harness
        .given_an_assistant_named("repo-explorer", &["Read"])
        .await;

    // When the listing path resolves them — `ListAgents` answers whoever asks
    let defs = harness
        .service
        .resolvable_agent_defs()
        .await
        .expect("the daemon must resolve its agent defs");

    // Then a key has no business in a fleet-wide listing
    assert_eq!(def_named(&defs, "repo-explorer").api_key, None);
}

#[tokio::test]
async fn the_registry_is_a_third_def_source_alongside_the_builtins() {
    // Given
    let harness = a_daemon_with_a_registry().await;
    harness
        .given_an_assistant_named("repo-explorer", &["Read"])
        .await;

    // When
    let defs = harness
        .service
        .resolvable_agent_defs()
        .await
        .expect("the daemon must resolve its agent defs");

    // Then
    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, vec!["fastcontext", "repo-explorer"]);
}

#[tokio::test]
async fn start_session_accepts_a_registry_assistant_as_its_agent() {
    // Given — an allowlist that does not contain the assistant's name
    let harness = a_daemon_with_a_registry().await;
    harness
        .given_an_assistant_named("repo-explorer", &["Read"])
        .await;

    // When
    let status = harness.when_starting_a_session_as("repo-explorer").await;

    // Then — the request got past the agent gate and failed on the project instead
    assert_eq!(status.code, Code::NotFound);
    assert_eq!(
        status.message,
        "project not found locally or on any peer: no-such-project"
    );
}

#[tokio::test]
async fn start_session_still_rejects_an_agent_that_is_neither_allowlisted_nor_an_assistant() {
    // Given
    let harness = a_daemon_with_a_registry().await;
    harness
        .given_an_assistant_named("repo-explorer", &["Read"])
        .await;

    // When
    let status = harness.when_starting_a_session_as("ghost-explorer").await;

    // Then
    assert_eq!(status.code, Code::InvalidArgument);
    assert!(
        status.message.contains("ghost-explorer"),
        "the refusal must name the agent that was asked for; got: {}",
        status.message
    );
}
