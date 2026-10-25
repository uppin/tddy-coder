//! Integration tests: a workspace session's seeded roster admits only the coding agents of the
//! daemon serving it.
//!
//! Feature: docs/ft/daemon/remote-codebase-mode.md § Coding subagents on the codebase host ("Who may be on a
//! workspace roster").
//!
//! A `StartSession` for a `workspace` session seeds its roster from `specialized_agents` before the
//! worktree is cut. Each reference resolves against this daemon or a peer in the common room, so a
//! model agent, or a coding agent another daemon owns, is seeded like any other entry. These tests
//! pin that the seed refuses both, naming the entry, creating nothing and asking the owning peer for
//! no checkout, and that a coding agent this daemon owns is still admitted.
//!
//! The common room is an in-process fake: a scripted laptop answers the one `ListSubagents` the seed
//! needs and records every call it is asked, so "no checkout was asked of it" is observable without
//! LiveKit. The session room is a fake that opens, so a clone's claim is not refused for want of a
//! room: that refusal would hide whether the seed admitted the agent.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use prost::Message as _;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_extensions::session_reach::{
    OpenedRoom, ReachableSessions, ReachedSession, SessionReach,
};
use tddy_daemon_extensions::PeerForwarder;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::operator_channel::NoOperatorChannel;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::catalog::{ListSubagentsResponse, SubagentInfo};
use tddy_service::proto::session::{
    SessionService as SessionServiceTrait, StartSessionRequest, StartSessionResponse,
};
use tddy_service::proto::session_agents_svc::{ListSessionAgentsRequest, SessionAgentService as _};
use tddy_service::proto::types::AgentKind;
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::test_util::{TestDaemon, TEST_TOKEN, TEST_USER};

/// This daemon: the codebase host the workspace session is started on.
const CODEBASE_HOST: &str = "codebase-host";
/// A peer in the common room, which owns the coding agent a seed must refuse.
const LAPTOP: &str = "laptop";
/// The coding agent the laptop lists.
const LAPTOPS_CODING_AGENT: &str = "codex-fast";
/// A coding agent this daemon offers, from its `allowed_agents`.
const LOCAL_CODING_AGENT: &str = "codex-acp";
/// The one call a seed makes to a peer: its listing of subagents.
const LISTING: (&str, &str) = ("catalog.CatalogService", "ListSubagents");
const PROJECT_ID: &str = "test-project";
const CHOSEN_SESSION_ID: &str = "019d105b-ac0f-78d3-9a89-409731145ab1";

const CONFIG_YAML: &str = r#"
daemon_instance_id: "codebase-host"
users:
  - github_user: "testuser"
    os_user: "testdev"
allowed_agents:
  - id: codex-acp
    label: "Codex ACP"
"#;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// A laptop in the common room. It lists one coding agent and answers nothing else. Every call it is
/// asked is recorded, so a test can see whether it was ever asked for a checkout.
#[derive(Default)]
struct ALaptopListingOneCodingAgent {
    carried: Mutex<Vec<(String, String, String)>>,
}

impl ALaptopListingOneCodingAgent {
    fn record(&self, peer_id: &str, service: &str, method: &str) {
        self.carried.lock().unwrap().push((
            peer_id.to_string(),
            service.to_string(),
            method.to_string(),
        ));
    }

    fn carried(&self) -> Vec<(String, String, String)> {
        self.carried.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl PeerForwarder for ALaptopListingOneCodingAgent {
    async fn is_connected(&self) -> bool {
        true
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        _body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        self.record(peer_id, service, method);
        if (service, method) != LISTING {
            return Err(Status::unavailable(format!(
                "{service}/{method} is not answered by this laptop"
            )));
        }
        Ok(ListSubagentsResponse {
            subagents: vec![SubagentInfo {
                name: LAPTOPS_CODING_AGENT.to_string(),
                agent_id: format!("{LAPTOPS_CODING_AGENT}@{LAPTOP}"),
                daemon_instance_id: LAPTOP.to_string(),
                kind: AgentKind::CodingAgent as i32,
                harness: "codex-acp".to_string(),
                ..Default::default()
            }],
        }
        .encode_to_vec())
    }

    async fn call_server_stream(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        _body: Vec<u8>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        self.record(peer_id, service, method);
        Err(Status::unavailable(format!(
            "{service}/{method} is not answered by this laptop"
        )))
    }
}

/// The common room this codebase host sits in: itself and the laptop.
struct ACommonRoomWithALaptop;

#[async_trait::async_trait]
impl EligibleDaemonSource for ACommonRoomWithALaptop {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        [CODEBASE_HOST, LAPTOP]
            .into_iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(id.to_string()),
                label: format!("{id} (common room)"),
            })
            .collect()
    }
}

/// A codebase host whose session room opens.
struct ASessionRoomThatOpens;

#[async_trait::async_trait]
impl SessionReach for ASessionRoomThatOpens {
    async fn open_now(
        &self,
        session: ReachedSession,
        _host: Arc<dyn ReachableSessions>,
    ) -> Result<Option<OpenedRoom>, Status> {
        Ok(Some(OpenedRoom {
            room: format!("session-{}", session.session_id),
            url: "ws://session-room.invalid".to_string(),
            server_identity: format!("daemon-{CODEBASE_HOST}"),
        }))
    }

    async fn session_reached(
        &self,
        _session: ReachedSession,
        _host: Arc<dyn ReachableSessions>,
    ) -> Result<(), Status> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A codebase host in a common room with a laptop, holding a project it can cut a worktree from.
struct ACodebaseHostInACommonRoom {
    daemon: TestDaemon,
    laptop: Arc<ALaptopListingOneCodingAgent>,
    sessions_base: PathBuf,
    _sessions: tempfile::TempDir,
    _repo: tempfile::TempDir,
}

impl ACodebaseHostInACommonRoom {
    fn session_dir_of(&self, session_id: &str) -> PathBuf {
        unified_session_dir_path(&self.sessions_base, session_id)
    }
}

fn a_codebase_host_in_a_common_room() -> ACodebaseHostInACommonRoom {
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    let repo = tempfile::tempdir().expect("repo tempdir");
    create_test_repo_with_origin(repo.path());
    register_project(sessions.path(), repo.path());

    let config_path = sessions.path().join("config.yaml");
    std::fs::write(&config_path, CONFIG_YAML).expect("write daemon config");
    let config = DaemonConfig::load(&config_path).expect("config must parse");

    let laptop = Arc::new(ALaptopListingOneCodingAgent::default());
    let peer_routing = PeerRouting::new(
        config.clone(),
        Arc::new(ACommonRoomWithALaptop),
        Some(CommonRoom::new(laptop.clone(), &config)),
    );

    let sessions_base = sessions.path().to_path_buf();
    let sessions_base_resolver: SessionsBaseResolver = {
        let base = sessions_base.clone();
        Arc::new(move |_| Some(base.clone()))
    };
    let user_resolver: SessionUserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| TEST_USER.to_string()));
    let host = DaemonSessionHost::new(
        config,
        sessions_base_resolver,
        sessions_base.clone(),
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        peer_routing,
        Arc::new(NoOperatorChannel),
        Arc::new(ClaudeCliSessionManager::new()),
    )
    .with_session_reach(Arc::new(ASessionRoomThatOpens));

    ACodebaseHostInACommonRoom {
        daemon: TestDaemon::from_arc(Arc::new(host)),
        laptop,
        sessions_base,
        _sessions: sessions,
        _repo: repo,
    }
}

/// A git repository whose `origin` is itself, so a worktree can be cut without a server.
fn create_test_repo_with_origin(dir: &Path) {
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git must be on PATH");
        assert!(status.success(), "git {args:?} must succeed in {dir:?}");
    };
    git(&["init", "-b", "main"]);
    git(&["config", "user.email", "t@t.com"]);
    git(&["config", "user.name", "Test"]);
    git(&["commit", "--allow-empty", "-m", "init"]);
    git(&["remote", "add", "origin", dir.to_str().expect("utf-8 path")]);
    git(&["push", "-u", "origin", "main"]);
}

fn register_project(sessions_base: &Path, repo_path: &Path) {
    let projects_dir = sessions_base.join("projects");
    std::fs::create_dir_all(&projects_dir).expect("create projects dir");
    let yaml = format!(
        "projects:\n  - project_id: {PROJECT_ID}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        repo_path.display()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).expect("write projects");
}

/// A model agent this daemon defines under `<tddyhome>/agents`. It replaces no tool: a model agent
/// that withdraws one is already refused on a session that cannot enforce the withdrawal, which is a
/// different ground from the one these tests pin.
fn a_model_agent_on_this_host(sessions_base: &Path, name: &str) {
    let agents = sessions_base.join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    std::fs::write(
        agents.join(format!("{name}.yaml")),
        format!("name: {name}\nmodel: qwen2.5-coder:7b\nbase_url: http://127.0.0.1:11434/v1\n"),
    )
    .expect("write agent def");
}

/// `StartSession` for a workspace session under `session_id`, seeded with `specialized_agents`.
async fn start_workspace_seeded_with(
    daemon: &TestDaemon,
    session_id: &str,
    specialized_agents: &[&str],
) -> Result<StartSessionResponse, Status> {
    daemon
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            session_type: "workspace".to_string(),
            requested_session_id: session_id.to_string(),
            specialized_agents: specialized_agents
                .iter()
                .map(|agent| agent.to_string())
                .collect(),
            ..Default::default()
        }))
        .await
        .map(|response| response.into_inner())
}

/// The qualified ids of the agents a session's roster holds.
async fn agent_ids_on_the_roster_of(daemon: &TestDaemon, session_id: &str) -> Vec<String> {
    daemon
        .session_agents_service()
        .list_session_agents(Request::direct(ListSessionAgentsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("listing a session's agents must succeed")
        .into_inner()
        .agents
        .into_iter()
        .map(|agent| agent.agent_id)
        .collect()
}

// ---------------------------------------------------------------------------
// Admission on the workspace seed path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_workspace_session_refuses_a_model_agent_of_this_daemon_naming_it() {
    // Given a model agent this daemon defines
    let host = a_codebase_host_in_a_common_room();
    a_model_agent_on_this_host(&host.sessions_base, "explorer");
    let agent = format!("explorer@{CODEBASE_HOST}");

    // When a workspace session is started with it
    let refusal = start_workspace_seeded_with(&host.daemon, CHOSEN_SESSION_ID, &[&agent])
        .await
        .expect_err("a model agent must not be seeded onto a workspace session");

    // Then the start is refused as a bad request naming the agent, and no session is created
    assert_eq!(
        refusal.code(),
        Code::InvalidArgument,
        "{}",
        refusal.message()
    );
    assert!(
        refusal.message().contains(&agent),
        "the refusal must name {agent}; got: {}",
        refusal.message()
    );
    assert!(
        !host.session_dir_of(CHOSEN_SESSION_ID).exists(),
        "a refused start must not create the session"
    );
}

#[tokio::test]
async fn a_workspace_session_refuses_a_coding_agent_of_another_daemon_naming_it() {
    // Given a coding agent the laptop owns, in the common room
    let host = a_codebase_host_in_a_common_room();
    let agent = format!("{LAPTOPS_CODING_AGENT}@{LAPTOP}");

    // When a workspace session is started with it
    let refusal = start_workspace_seeded_with(&host.daemon, CHOSEN_SESSION_ID, &[&agent])
        .await
        .expect_err(
            "a coding agent another daemon owns must not be seeded onto a workspace session",
        );

    // Then the start is refused naming the agent, no session is created, and the laptop was asked
    // only for its listing: no checkout was asked of it, so none was claimed
    assert_eq!(
        refusal.code(),
        Code::InvalidArgument,
        "{}",
        refusal.message()
    );
    assert!(
        refusal.message().contains(&agent),
        "the refusal must name {agent}; got: {}",
        refusal.message()
    );
    assert!(
        !host.session_dir_of(CHOSEN_SESSION_ID).exists(),
        "a refused start must not create the session"
    );
    assert_eq!(
        host.laptop.carried(),
        vec![(
            LAPTOP.to_string(),
            LISTING.0.to_string(),
            LISTING.1.to_string()
        )],
        "the laptop must be asked only for its listing, never for a checkout"
    );
}

#[tokio::test]
async fn a_workspace_session_admits_a_coding_agent_of_this_daemon_and_records_it() {
    // Given a coding agent this daemon offers
    let host = a_codebase_host_in_a_common_room();
    let agent = format!("{LOCAL_CODING_AGENT}@{CODEBASE_HOST}");

    // When a workspace session is started with it
    start_workspace_seeded_with(&host.daemon, CHOSEN_SESSION_ID, &[&agent])
        .await
        .expect("a coding agent this daemon owns must be admitted");

    // Then the session's roster holds it
    assert_eq!(
        agent_ids_on_the_roster_of(&host.daemon, CHOSEN_SESSION_ID).await,
        vec![agent]
    );
}
