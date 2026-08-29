//! Acceptance tests: agents owned by another daemon — resolution, room admission, clones, tool
//! routing and teardown.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC26-AC43)
//!
//! Two real daemons in a real common room, with a real git project. Nothing here is decidable on
//! one host: every assertion is about *which host answered*, and a single-daemon fixture would
//! answer every one of them from the wrong place while passing.
//!
//! `#[serial]` and the LiveKit testkit, following `multi_host_acceptance.rs` — these bind a shared
//! room and a Docker-backed server, so they cannot interleave.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::RoomOptions;
use pretty_assertions::assert_eq;
use serial_test::serial;
use tddy_connectrpc::connect_router;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::remote_git_service::{ProjectsDirResolver, RemoteGitServiceImpl};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_livekit::LiveKitParticipant;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::{Code, MultiRpcService, Request, RpcBridge, RpcService, ServiceEntry};
use tddy_service::proto::connection::{
    AttachSessionAgentRequest, CancelAgentConversationRequest,
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest, DetachSessionAgentRequest,
    ExecuteToolRequest, ListSessionAgentsRequest, ListSessionsRequest,
    OpenAgentConversationRequest, PromptAgentConversationRequest, SessionAgentRoster,
    StartSessionRequest,
};
use tddy_service::{
    LiveKitTokenServiceServer, RemoteGitServiceServer, SessionAdmissionServiceServer,
};

const ROOM: &str = "agent-roster-common-room";
const DAEMON_B: &str = "agent-roster-daemon-b";
const DAEMON_C: &str = "agent-roster-daemon-c";
const LK_API_KEY: &str = "devkey";
const LK_API_SECRET: &str = "secret";
const PROJECT_ID: &str = "agent-roster-project";

/// Clone state values from `AgentCloneState`, as the proto numbers them.
const CLONE_STATE_LOCAL: i32 = 1;
const CLONE_STATE_PROVISIONING: i32 = 2;
const CLONE_STATE_READY: i32 = 3;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A facilitating daemon A with a live session, plus zero or more peer daemons in the same room.
struct Fleet {
    a: ConnectionServiceImpl,
    session_id: String,
    peers: Vec<PeerDaemon>,
    _livekit: LiveKitTestkit,
    sessions_a: tempfile::TempDir,
    project: tempfile::TempDir,
    _configs: Vec<tempfile::TempDir>,
    /// Per-daemon tempdirs used as `repos_base_path` so facilitator clones land in a tempdir that is
    /// cleaned up, not the operator's real home (PRD AC37). Held for the fleet's lifetime.
    _repos_bases: Vec<tempfile::TempDir>,
    /// A's Connect-HTTP surface serving `auth.LiveKitTokenService` — the address
    /// `tddy-remote-git-repo` mints a room token from before it opens a `RemoteGitService` stream
    /// (PRD AC37). Held so it lives as long as the fleet.
    _daemon_http_a: BackgroundTask,
    /// A's RPC LiveKit participant serving `remote_git.RemoteGitGitService` on the common room, so a
    /// peer that has never seen the project can clone it from A (PRD AC37).
    _rpc_participant_a: BackgroundTask,
}

/// A server task that lives exactly as long as the fixture that started it.
struct BackgroundTask(tokio::task::JoinHandle<()>);

impl Drop for BackgroundTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct PeerDaemon {
    instance_id: String,
    sessions: tempfile::TempDir,
    /// The peer's own service, so a test can address its RPC surface directly — which is what any
    /// common-room participant able to reach that daemon can do.
    service: ConnectionServiceImpl,
    run: tokio::task::JoinHandle<()>,
}

impl Fleet {
    async fn attach(&self, agent_id: &str) -> Result<SessionAgentRoster, tddy_rpc::Status> {
        self.a
            .attach_session_agent(Request::new(AttachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn detach(&self, agent_id: &str) -> Result<SessionAgentRoster, tddy_rpc::Status> {
        self.a
            .detach_session_agent(Request::new(DetachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn roster(&self) -> SessionAgentRoster {
        self.a
            .list_session_agents(Request::new(ListSessionAgentsRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("listing the roster must succeed")
            .into_inner()
    }

    /// Wait until the entry's clone reports ready, so a test asserting on the clone is not racing
    /// its provisioning. Bounded — a clone that never finishes is a failure, not a hang.
    async fn await_clone_ready(&self, agent_id: &str) -> String {
        // 90s: provisioning may `git clone` the project onto the peer before building a worktree.
        let result = tokio::time::timeout(Duration::from_secs(90), async {
            loop {
                let roster = self.roster().await;
                let entry = roster
                    .agents
                    .iter()
                    .find(|a| a.agent_id == agent_id)
                    .unwrap_or_else(|| panic!("roster lost '{agent_id}' while provisioning"));
                if entry.clone_state == CLONE_STATE_READY {
                    return entry.codebase_session_id.clone();
                }
                if entry.clone_state == CLONE_STATE_LOCAL {
                    eprintln!("[diag] clone_state=LOCAL error={}", entry.clone_error);
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await;
        match result {
            Ok(id) => id,
            Err(_) => {
                let roster = self.roster().await;
                let entry = roster.agents.iter().find(|a| a.agent_id == agent_id);
                panic!(
                    "clone for '{agent_id}' never became ready — last state={:?} error={}",
                    entry.map(|e| e.clone_state),
                    entry.map(|e| e.clone_error.clone()).unwrap_or_default(),
                );
            }
        }
    }

    /// An open conversation with `agent_id`, ready to be prompted.
    async fn a_conversation_with(&self, agent_id: &str) -> String {
        self.a
            .open_agent_conversation(Request::new(OpenAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
                conversation_id: String::new(),
            }))
            .await
            .expect("opening a conversation with a remote agent must succeed")
            .into_inner()
            .conversation_id
    }

    fn peer(&self, instance_id: &str) -> &PeerDaemon {
        self.peers
            .iter()
            .find(|p| p.instance_id == instance_id)
            .unwrap_or_else(|| panic!("no peer daemon '{instance_id}' in this fleet"))
    }

    /// The roster daemon A persisted for one of its own sessions.
    fn roster_of(&self, session_id: &str) -> Vec<tddy_core::SessionAgentRecord> {
        let session_dir = unified_session_dir_path(self.sessions_a.path(), session_id);
        tddy_core::read_session_metadata(&session_dir)
            .expect("the started session has metadata on A")
            .agents
    }

    /// The main agent's own worktree on A — the one every mutation must land in.
    fn authoritative_worktree(&self) -> PathBuf {
        self.project.path().to_path_buf()
    }
}

/// Stand up daemon A with a managed session, and one peer daemon per `(instance_id, agents)` pair.
/// Each peer's agents directory holds the named defs, pointed at `model_base_url`.
async fn a_fleet_with_peers(peers: &[(&str, &[&str])], model_base_url: &str) -> Fleet {
    assert!(
        !model_base_url.is_empty(),
        "every def in the fleet is pointed at this endpoint; an empty one makes an agent's first \
         turn fail on a URL nobody serves"
    );
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let ws_url = livekit.get_ws_url();
    let os_user = std::env::var("USER").expect("USER required for spawn identity");

    let project = tempfile::tempdir().expect("project tempdir");
    a_git_project(project.path());

    let user_resolver: UserResolver = Arc::new(|token| {
        if token == TEST_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });

    let mut configs = Vec::new();
    let mut running_peers = Vec::new();
    let mut repos_bases = Vec::new();
    for (instance_id, agents) in peers {
        let repos_base = tempfile::tempdir().expect("peer repos_base tempdir");
        let repos_base_path = repos_base.path().to_string_lossy().into_owned();
        repos_bases.push(repos_base);
        let (cfg_dir, cfg_path) =
            write_daemon_yaml(&ws_url, Some(instance_id), &os_user, &repos_base_path);
        let config = DaemonConfig::load(&cfg_path).expect("peer config");
        configs.push(cfg_dir);

        let sessions = tempfile::tempdir().expect("peer sessions tempdir");
        write_project_registry(sessions.path(), project.path());
        write_agent_defs(sessions.path(), agents, model_base_url);

        let base = sessions.path().to_path_buf();
        let resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        let service = ConnectionServiceImpl::new(
            config.clone(),
            resolver,
            sessions.path().to_path_buf(),
            user_resolver.clone(),
            None,
            None,
            None,
            Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
        );

        tddy_daemon::livekit_peer_discovery::spawn_common_room_discovery_task(
            Arc::new(config),
            Arc::new(tddy_daemon::livekit_peer_discovery::CommonRoomPeerRegistry::new()),
            Arc::new(tokio::sync::RwLock::new(None)),
        );

        let token = livekit
            .generate_token(ROOM, &format!("daemon-{instance_id}"))
            .expect("LiveKit token for peer daemon");
        let participant = LiveKitParticipant::connect(
            &ws_url,
            &token,
            tddy_service::ConnectionServiceServer::new(service.clone()),
            RoomOptions::default(),
            None,
            None,
        )
        .await
        .expect("peer daemon joins the common room");
        let run = tokio::spawn(async move {
            let _ = participant.run().await;
        });

        running_peers.push(PeerDaemon {
            instance_id: instance_id.to_string(),
            sessions,
            service,
            run,
        });
    }

    // Daemon A — the facilitating daemon, with peer discovery wired so it can resolve remote ids.
    let repos_base_a = tempfile::tempdir().expect("daemon A repos_base tempdir");
    let repos_base_path_a = repos_base_a.path().to_string_lossy().into_owned();
    repos_bases.push(repos_base_a);
    let (cfg_dir_a, cfg_path_a) = write_daemon_yaml(&ws_url, None, &os_user, &repos_base_path_a);
    let config_a = DaemonConfig::load(&cfg_path_a).expect("daemon A config");
    configs.push(cfg_dir_a);

    let sessions_a = tempfile::tempdir().expect("sessions A tempdir");
    write_project_registry(sessions_a.path(), project.path());
    let base_a = sessions_a.path().to_path_buf();
    let resolver_a: SessionsBaseResolver = Arc::new(move |_| Some(base_a.clone()));

    // A's Connect-HTTP surface serving `auth.LiveKitTokenService` — the address a peer's
    // `tddy-remote-git-repo` mints a common-room token from before it opens a `RemoteGitService`
    // stream on A (PRD AC37). Built before `ConnectionServiceImpl` so the same `daemon_url` can be
    // advertised to peers via `listen.advertise_url`.
    //
    // The mint's resolver is the fleet's own `user_resolver` (the same one `ConnectionServiceImpl`
    // uses), not the HMAC verifier `auth::build_auth_entries` would build — the fleet authenticates
    // with the plain `TEST_TOKEN`, which is not a signed token, so the HMAC path would refuse it.
    let livekit_token_entry = tddy_rpc::ServiceEntry {
        name: "auth.LiveKitTokenService",
        service: Arc::new(LiveKitTokenServiceServer::new(
            tddy_daemon::auth::LiveKitTokenServiceImpl::new(
                user_resolver.clone(),
                Arc::new(config_a.clone()),
            ),
        )) as Arc<dyn RpcService>,
    };
    let daemon_http_router_a = connect_router(RpcBridge::new(MultiRpcService::new(vec![
        livekit_token_entry,
    ])));
    let daemon_http_listener_a = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind A's HTTP surface");
    let daemon_url_a = format!(
        "http://{}",
        daemon_http_listener_a
            .local_addr()
            .expect("A HTTP local addr")
    );
    let mut config_a = config_a;
    config_a.listen.advertise_url = Some(daemon_url_a.clone());
    let daemon_http_a = BackgroundTask(tokio::spawn(async move {
        let _ = axum::serve(daemon_http_listener_a, daemon_http_router_a).await;
    }));

    let config_arc = Arc::new(config_a.clone());
    let registry = Arc::new(tddy_daemon::livekit_peer_discovery::CommonRoomPeerRegistry::new());
    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
    tddy_daemon::livekit_peer_discovery::spawn_common_room_discovery_task(
        config_arc.clone(),
        registry.clone(),
        room_slot.clone(),
    );
    let eligible: Arc<dyn tddy_daemon::multi_host::EligibleDaemonSource> = Arc::new(
        tddy_daemon::livekit_peer_discovery::LiveKitEligibleDaemonSource::new(
            config_arc,
            registry,
            room_slot.clone(),
        ),
    );
    let service_a = ConnectionServiceImpl::new(
        config_a.clone(),
        resolver_a,
        sessions_a.path().to_path_buf(),
        user_resolver.clone(),
        None,
        Some(
            tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles {
                eligible_daemon_source: eligible,
                common_room_livekit_room: room_slot,
            },
        ),
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    );

    // A's RPC LiveKit participant serving `remote_git.RemoteGitService` and
    // `session_admission.SessionAdmissionService` on the common room. The remote-git service lets a
    // peer that has never seen the project `git clone` it from A (PRD AC37); the admission service
    // is the re-admit handshake's token mint (PRD § "What attach does" step 3) — an owning daemon
    // whose short-TTL admission token is expiring calls `AdmitOwningDaemon` here for a fresh one.
    // Both joined under the `daemon-`-prefixed identity `provision_agent_clone` advertises as the
    // facilitating daemon, and both share the admission registry `service_a` records against, so a
    // revoke on the last detach is visible to the next re-admit call.
    let projects_dir_a = sessions_a.path().join("projects");
    let remote_git_resolver: ProjectsDirResolver = {
        let dir = projects_dir_a.clone();
        let serving = os_user.clone();
        Arc::new(move |os_user| (os_user == serving).then(|| dir.clone()))
    };
    let session_admissions = service_a.session_admissions();
    let session_rooms_for_admissions = service_a.session_rooms();
    let remote_git_server = RemoteGitServiceServer::new(RemoteGitServiceImpl::new(
        user_resolver.clone(),
        remote_git_resolver,
        Arc::new(config_a.clone()),
    ));
    let admission_server = SessionAdmissionServiceServer::new(
        tddy_daemon::session_admission_service::SessionAdmissionServiceImpl::new(
            user_resolver,
            Arc::new(config_a.clone()),
            session_admissions,
            Arc::new({
                let rooms = session_rooms_for_admissions;
                move |session_id: &str| rooms.contains(session_id)
            }),
        ),
    );
    let lk_multi_a = MultiRpcService::new(vec![
        ServiceEntry {
            name: "remote_git.RemoteGitService",
            service: Arc::new(remote_git_server) as Arc<dyn RpcService>,
        },
        ServiceEntry {
            name: "session_admission.SessionAdmissionService",
            service: Arc::new(admission_server) as Arc<dyn RpcService>,
        },
    ]);
    let instance_id_a =
        tddy_daemon::livekit_peer_discovery::local_instance_id_for_config(&config_a);
    let rpc_identity_a = tddy_daemon::livekit_peer_discovery::daemon_rpc_identity(&instance_id_a);
    let rpc_token_a = livekit
        .generate_token(ROOM, &rpc_identity_a)
        .expect("A RPC participant token");
    let rpc_participant_a = LiveKitParticipant::connect(
        &ws_url,
        &rpc_token_a,
        lk_multi_a,
        RoomOptions::default(),
        None,
        None,
    )
    .await
    .expect("A's RPC participant joins the common room");
    let rpc_run_a = BackgroundTask(tokio::spawn(async move {
        let _ = rpc_participant_a.run().await;
    }));

    let session_id = "1780828020298-remote-roster".to_string();
    let session_dir = unified_session_dir_path(sessions_a.path(), &session_id);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    tddy_core::write_session_metadata(
        &session_dir,
        &a_managed_session(&session_id, project.path()),
    )
    .expect("write session metadata");

    let fleet = Fleet {
        a: service_a,
        session_id,
        peers: running_peers,
        _livekit: livekit,
        sessions_a,
        project,
        _configs: configs,
        _repos_bases: repos_bases,
        _daemon_http_a: daemon_http_a,
        _rpc_participant_a: rpc_run_a,
    };
    fleet.await_peers_discovered(peers.len()).await;
    fleet
}

impl Fleet {
    /// Block until A has discovered every peer, so an attach is not racing discovery.
    async fn await_peers_discovered(&self, expected: usize) {
        // 45s: matches the discovery wait the existing multi-host suite uses.
        tokio::time::timeout(Duration::from_secs(45), async {
            loop {
                let daemons = self
                    .a
                    .list_eligible_daemons(Request::new(
                        tddy_service::proto::connection::ListEligibleDaemonsRequest {
                            session_token: TEST_TOKEN.to_string(),
                        },
                    ))
                    .await
                    .expect("ListEligibleDaemons")
                    .into_inner()
                    .daemons;
                let discovered = self
                    .peers
                    .iter()
                    .filter(|p| daemons.iter().any(|d| d.instance_id == p.instance_id))
                    .count();
                if discovered == expected {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        })
        .await
        .expect("timeout waiting for peer daemons to be discovered");
    }
}

/// A stub `cursor-agent` that mints a chat id and then idles on stdin, so a cursor-cli start can
/// reach its launch on a host where no real Cursor CLI is installed.
fn write_stub_cursor_agent(dir: &Path) -> PathBuf {
    let script_path = dir.join("stub_cursor_agent.sh");
    std::fs::write(
        &script_path,
        "#!/bin/sh\nif [ \"$1\" = \"create-chat\" ]; then echo stub-chat-id; exit 0; fi\ncat\n",
    )
    .expect("write stub cursor-agent");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
        .expect("make the stub cursor-agent executable");
    script_path
}

fn write_daemon_yaml(
    ws_url: &str,
    instance_id: Option<&str>,
    os_user: &str,
    repos_base_path: &str,
) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("config tempdir");
    let path = dir.path().join("daemon.yaml");
    let cursor_binary = write_stub_cursor_agent(dir.path());
    let cursor_binary = cursor_binary.display();
    let id_block = instance_id
        .map(|id| format!("daemon_instance_id: {id}\n"))
        .unwrap_or_default();
    let true_path = if cfg!(target_os = "macos") {
        "/usr/bin/true"
    } else {
        "/bin/true"
    };
    std::fs::write(
        &path,
        format!(
            r#"
{id_block}users:
  - github_user: "testuser"
    os_user: "{os_user}"
repos_base_path: {repos_base_path}
allowed_tools:
  - path: {true_path}
    label: t
cursor_cli:
  binary_path: {cursor_binary}
livekit:
  url: {ws_url}
  api_key: {LK_API_KEY}
  api_secret: {LK_API_SECRET}
  common_room: {ROOM}
"#
        ),
    )
    .expect("write daemon.yaml");
    (dir, path)
}

fn write_project_registry(tddy_data_dir: &Path, repo_path: &Path) {
    let projects_dir = tddy_data_dir.join("projects");
    tddy_daemon::project_storage::write_projects(
        &projects_dir,
        &[tddy_daemon::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "agent-roster".to_string(),
            git_url: "https://example.invalid/agent-roster.git".to_string(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: HashMap::new(),
        }],
    )
    .expect("write projects registry");
}

/// Every def binds the read trio and `WRITE`, so a test can drive either half of the read/write
/// split by scripting the model to call the tool it wants. Binding a tool is not using it: an agent
/// only ever runs the calls its model issues.
fn write_agent_defs(tddy_data_dir: &Path, agents: &[&str], model_base_url: &str) {
    if agents.is_empty() {
        return;
    }
    let agents_dir = tddy_data_dir.join("agents");
    std::fs::create_dir_all(&agents_dir).expect("create agents dir");
    for name in agents {
        std::fs::write(
            agents_dir.join(format!("{name}.yaml")),
            format!(
                "name: {name}\nlabel: \"{name}\"\nmodel: stub-model\n\
                 base_url: {model_base_url}\ntools: [READ, GLOB, GREP, WRITE]\nreplaces: []\n"
            ),
        )
        .expect("write agent def");
    }
}

fn a_git_project(root: &Path) {
    git(root, &["init", "-q", "--initial-branch=main"]);
    git(root, &["config", "user.email", "agent@example.com"]);
    git(root, &["config", "user.name", "Agent"]);
    std::fs::write(root.join("README.md"), "# agent roster fixture\n").expect("write README");
    std::fs::create_dir_all(root.join("src")).expect("create src");
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write source");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "initial"]);
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn a_managed_session(session_id: &str, repo_path: &Path) -> SessionMetadata {
    SessionMetadata {
        session_id: session_id.to_string(),
        project_id: PROJECT_ID.to_string(),
        created_at: "2026-08-16T10:00:00Z".to_string(),
        updated_at: "2026-08-16T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(repo_path.display().to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: None,
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: Some(true),
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
    }
}

/// A start whose codebase is on the daemon it is addressed to — the ordinary shape, and the one a
/// split start is the exception to. `cursor-cli` unsandboxed, because that is the co-located launch
/// whose seed is resolved on the daemon's own start path rather than inside a jail helper.
/// A start whose codebase is the daemon it is addressed to — no split, no `placement`, just this
/// host's own checkout — naming one agent for its roster.
fn a_co_located_start_seeding(agent_id: &str, repo_path: &Path) -> StartSessionRequest {
    StartSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        session_type: "cursor-cli".to_string(),
        project_id: PROJECT_ID.to_string(),
        repo_path: repo_path.to_string_lossy().into_owned(),
        model: "stub-model".to_string(),
        managed_codebase: true,
        specialized_agents: vec![agent_id.to_string()],
        ..Default::default()
    }
}

/// One scripted turn of the stub model: prose the agent's loop hands back as its answer, or a tool
/// call the loop must dispatch before it asks again.
enum ModelTurn {
    Prose(String),
    ToolCall {
        tool: String,
        args: serde_json::Value,
    },
}

impl ModelTurn {
    /// The `chat/completions` body an OpenAI-compatible endpoint answers this turn with.
    fn response_body(&self) -> String {
        let message = match self {
            ModelTurn::Prose(content) => serde_json::json!({
                "role": "assistant", "content": content
            }),
            ModelTurn::ToolCall { tool, args } => serde_json::json!({
                "role": "assistant",
                "content": serde_json::Value::Null,
                "tool_calls": [{
                    "id": format!("call-{tool}"),
                    "type": "function",
                    "function": { "name": tool, "arguments": args.to_string() },
                }],
            }),
        };
        let finish_reason = match self {
            ModelTurn::Prose(_) => "stop",
            ModelTurn::ToolCall { .. } => "tool_calls",
        };
        serde_json::json!({
            "choices": [{ "message": message, "finish_reason": finish_reason }]
        })
        .to_string()
    }
}

struct StubModelBuilder {
    turns: Vec<ModelTurn>,
}

/// A stub model with no turns yet — script at least one before starting it.
fn a_stub_model() -> StubModelBuilder {
    StubModelBuilder { turns: Vec::new() }
}

impl StubModelBuilder {
    /// A turn of plain prose. With no tool call and no `<final_answer>`, the loop treats it as the
    /// agent's answer and ends the turn.
    fn saying(mut self, content: &str) -> Self {
        self.turns.push(ModelTurn::Prose(content.to_string()));
        self
    }

    /// A turn that calls `tool`, which the agent's own loop dispatches against its codebase access
    /// before asking the model again.
    fn calling(mut self, tool: &str, args: serde_json::Value) -> Self {
        self.turns.push(ModelTurn::ToolCall {
            tool: tool.to_string(),
            args,
        });
        self
    }

    async fn start(self) -> StubModel {
        assert!(
            !self.turns.is_empty(),
            "a stub model must be scripted with at least one turn, or the agent's first call hangs \
             on an endpoint with nothing to say"
        );
        StubModel::serving(self.turns).await
    }
}

/// An OpenAI-compatible chat-completions endpoint that answers a scripted sequence of turns and
/// keeps every request it was sent. Hand-rolled rather than a mock-server dependency: it serves
/// exactly one shape and its whole contract is visible here.
///
/// The recorded requests are the only place a test can see what a *tool call* returned — the
/// conversation stream carries the model's final answer and nothing else, while the loop feeds each
/// tool result back as a `tool`-role message on the next request.
struct StubModel {
    base_url: String,
    requests: Arc<std::sync::Mutex<Vec<String>>>,
    serving: tokio::task::JoinHandle<()>,
}

/// The listener goes with the test that scripted it — twenty tests in one process must not leave
/// twenty endpoints accepting connections behind them.
impl Drop for StubModel {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

impl StubModel {
    async fn serving(turns: Vec<ModelTurn>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub model server");
        let port = listener.local_addr().expect("stub model addr").port();
        let requests: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(vec![]));

        let recorded = Arc::clone(&requests);
        let serving = tokio::spawn(async move {
            let mut answered = 0usize;
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let request = read_http_request(&mut socket).await;
                recorded
                    .lock()
                    .expect("stub model request log")
                    .push(request);
                // A loop that keeps asking past the script gets the last turn again, so an
                // over-run ends the conversation instead of hanging on a closed socket.
                let turn = &turns[answered.min(turns.len() - 1)];
                answered += 1;
                let body = turn.response_body();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                use tokio::io::AsyncWriteExt;
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        StubModel {
            base_url: format!("http://127.0.0.1:{port}"),
            requests,
            serving,
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The tool results the agent's loop fed back into the conversation.
    ///
    /// Taken from the last request the model was sent, which carries the whole history: a result
    /// the loop never produced is a result no later request can contain.
    fn tool_results(&self) -> Vec<String> {
        let requests = self.requests.lock().expect("stub model request log");
        let last = requests
            .last()
            .expect("the agent's turn loop never called the model");
        let body = last
            .split_once("\r\n\r\n")
            .expect("a chat-completions request must carry a body")
            .1;
        let request: serde_json::Value =
            serde_json::from_str(body).expect("a chat-completions body must be JSON");
        request["messages"]
            .as_array()
            .expect("a chat-completions body must carry messages")
            .iter()
            .filter(|message| message["role"] == "tool")
            .map(|message| message["content"].as_str().unwrap_or_default().to_string())
            .collect()
    }
}

/// Read one whole HTTP/1.1 request — headers and the body its `Content-Length` announces — so a
/// recorded request is the entire chat-completion call rather than however much of it arrived
/// first.
async fn read_http_request(socket: &mut tokio::net::TcpStream) -> String {
    use tokio::io::AsyncReadExt;
    let mut raw: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = socket.read(&mut chunk).await.unwrap_or(0);
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
        if let Some(body_start) = header_end(&raw) {
            if raw.len() - body_start >= content_length(&raw[..body_start]) {
                break;
            }
        }
    }
    String::from_utf8_lossy(&raw).into_owned()
}

/// Index of the first body byte, once the blank line that ends the headers has arrived.
fn header_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4)
}

/// The announced body length, or zero for a request that declares none.
fn content_length(headers: &[u8]) -> usize {
    String::from_utf8_lossy(headers)
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait RosterAssertions {
    fn entry(&self, agent_id: &str) -> &tddy_service::proto::connection::SessionAgentEntry;
}

impl RosterAssertions for SessionAgentRoster {
    fn entry(&self, agent_id: &str) -> &tddy_service::proto::connection::SessionAgentEntry {
        self.agents
            .iter()
            .find(|a| a.agent_id == agent_id)
            .unwrap_or_else(|| panic!("roster has no entry for '{agent_id}'"))
    }
}

/// The session ids a peer daemon currently holds.
async fn peer_session_ids(fleet: &Fleet, instance_id: &str) -> Vec<String> {
    let peer = fleet.peer(instance_id);
    let sessions_root = peer.sessions.path().join("sessions");
    if !sessions_root.is_dir() {
        return Vec::new();
    }
    let mut ids: Vec<String> = std::fs::read_dir(&sessions_root)
        .expect("read peer sessions dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    ids
}

/// Where an agent runs decides how a session is split across hosts, never whether it can be named.
/// A session whose codebase is on this daemon reaches a peer's agent the same way a split one does
/// — over a synced clone — so the co-located start must take the seed rather than refuse it.
#[tokio::test]
#[serial]
async fn seeds_a_peers_agent_onto_a_session_whose_codebase_is_here() {
    // Given a peer that owns `explorer`, and an ordinary co-located start naming it
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");

    // When
    let started = fleet
        .a
        .start_session(Request::new(a_co_located_start_seeding(
            &explorer,
            &fleet.authoritative_worktree(),
        )))
        .await
        .expect("a co-located start may name an agent another daemon owns")
        .into_inner();

    // Then the roster it persisted names the peer's agent, pointed at the clone that peer will
    // build — the same entry a split start writes, for a session that was never split
    let seeded = fleet.roster_of(&started.session_id);
    assert_eq!(
        seeded
            .iter()
            .map(|record| record.agent_id.as_str())
            .collect::<Vec<_>>(),
        vec![explorer.as_str()],
        "the start's roster should name the agent it was asked for"
    );
    assert_eq!(
        seeded[0].daemon_instance_id, DAEMON_B,
        "the agent runs where its def lives"
    );
    assert!(
        seeded[0].codebase_session_id.is_some(),
        "an agent on another host reads a clone of this checkout, and the roster is what names it"
    );
}

// ---------------------------------------------------------------------------
// AC26-AC28 — a remote agent resolves, joins, and answers
// ---------------------------------------------------------------------------

/// A def that exists only on B resolves through B, and the entry records B as its owner. Resolving
/// it locally would either fail (the def is not here) or, worse, match a same-named local def.
#[tokio::test]
#[serial]
async fn resolves_a_remote_agent_from_the_daemon_that_defines_it() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");

    // When
    let roster = fleet
        .attach(&explorer)
        .await
        .expect("a remote agent must attach");

    // Then
    let entry = roster.entry(&explorer);
    assert_eq!(entry.daemon_instance_id, DAEMON_B);
    assert_eq!(entry.name, "explorer");
    assert_ne!(
        entry.clone_state, CLONE_STATE_LOCAL,
        "a remote agent must not be recorded as locally served"
    );
}

/// The owning daemon joins the session's own room. Before this, `session-{id}` had exactly one
/// daemon participant; a remote agent's conversations ride the second one.
#[tokio::test]
#[serial]
async fn brings_the_owning_daemon_into_the_session_room() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");

    // When
    fleet.attach(&explorer).await.expect("attach remote agent");
    fleet.await_clone_ready(&explorer).await;

    // Then
    let participants = fleet
        .a
        .session_room_participant_identities(&fleet.session_id)
        .await
        .expect("reading the session room's participants must succeed");
    assert!(
        participants
            .iter()
            .any(|p| p == &format!("daemon-{DAEMON_B}")),
        "the owning daemon must be a participant of session-{}; saw {participants:?}",
        fleet.session_id
    );
}

/// A remote agent's answer has the same shape a local one's does. The main agent addresses a
/// qualified id and gets `{stopReason, content}` — it cannot tell which host ran the loop, which is
/// the property that makes remote agents usable at all.
#[tokio::test]
#[serial]
async fn answers_a_prompt_to_a_remote_agent_the_way_a_local_one_answers() {
    // Given
    let model = a_stub_model()
        .saying("<final_answer>src/main.rs</final_answer>")
        .start()
        .await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach remote agent");
    fleet.await_clone_ready(&explorer).await;

    // When
    let conversation = open_conversation_with(&fleet, &explorer).await;
    let answer = collect_prompt(&fleet, &conversation, "where is main?").await;

    // Then
    assert_eq!(answer.stop_reason, "EndTurn");
    assert!(
        answer.content.contains("src/main.rs"),
        "the remote agent's answer must reach the caller intact, was: {}",
        answer.content
    );
}

/// A cancel has to reach the daemon the turn is *running* on, which is not the daemon holding the
/// roster. The forward is re-addressed to the conversation's owning daemon for that reason: one that
/// still named the roster host would arrive at the peer, be classified as belonging elsewhere, and be
/// routed straight back — the two daemons handing the same cancel to each other while the turn runs
/// on regardless. Nothing about that is visible to the caller, which is told the turn was cancelled.
#[tokio::test]
#[serial]
async fn cancels_a_remote_agents_conversation_on_the_daemon_that_owns_it() {
    // Given
    let model = a_stub_model()
        .saying("<final_answer>src/main.rs</final_answer>")
        .start()
        .await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach remote agent");
    fleet.await_clone_ready(&explorer).await;
    let conversation = open_conversation_with(&fleet, &explorer).await;

    // When
    fleet
        .a
        .cancel_agent_conversation(Request::new(CancelAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: fleet.session_id.clone(),
            daemon_instance_id: String::new(),
            conversation_id: conversation.clone(),
        }))
        .await
        .expect("cancelling a remote conversation must reach the daemon running the turn");

    // Then — the id is gone on both sides, so a later prompt is refused rather than answered by a
    // conversation the caller was told was cancelled
    let status = fleet
        .a
        .prompt_agent_conversation(Request::new(PromptAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: fleet.session_id.clone(),
            daemon_instance_id: String::new(),
            conversation_id: conversation.clone(),
            prompt: "where is main?".to_string(),
        }))
        .await
        .expect_err("a cancelled conversation must not be promptable");
    assert_eq!(status.code(), Code::NotFound);
}

struct PromptAnswer {
    stop_reason: String,
    content: String,
}

/// Open a conversation with `agent_id` through daemon A, the way the main agent does.
async fn open_conversation_with(fleet: &Fleet, agent_id: &str) -> String {
    fleet
        .a
        .open_agent_conversation(Request::new(OpenAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: fleet.session_id.clone(),
            daemon_instance_id: String::new(),
            agent_id: agent_id.to_string(),
            conversation_id: String::new(),
        }))
        .await
        .expect("opening a conversation with a remote agent must succeed")
        .into_inner()
        .conversation_id
}

async fn collect_prompt(fleet: &Fleet, conversation_id: &str, prompt: &str) -> PromptAnswer {
    use futures_util::StreamExt;
    let mut stream = fleet
        .a
        .prompt_agent_conversation(Request::new(PromptAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: fleet.session_id.clone(),
            daemon_instance_id: String::new(),
            conversation_id: conversation_id.to_string(),
            prompt: prompt.to_string(),
        }))
        .await
        .expect("prompting a remote agent must succeed")
        .into_inner();

    let mut content = String::new();
    let mut stop_reason = String::new();
    // 60s: one model round trip over the peer link, well inside the stub's latency.
    let collected = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(frame) = stream.next().await {
            let frame = frame.expect("the conversation stream must not error");
            content.push_str(&frame.content_chunk);
            if frame.last {
                stop_reason = frame.stop_reason.clone();
                break;
            }
        }
    })
    .await;
    collected.expect("the remote agent never finished its turn");
    PromptAnswer {
        stop_reason,
        content,
    }
}

// ---------------------------------------------------------------------------
// AC29-AC30 — one clone per (session, daemon)
// ---------------------------------------------------------------------------

/// Two agents on one host share one checkout. A checkout each would multiply disk and sync cost for
/// isolation a read-only mirror does not need.
#[tokio::test]
#[serial]
async fn serves_two_agents_of_one_daemon_from_a_single_clone() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer", "linter"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    let linter = format!("linter@{DAEMON_B}");

    // When
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.attach(&linter).await.expect("attach linter");
    let explorer_clone = fleet.await_clone_ready(&explorer).await;
    let linter_clone = fleet.await_clone_ready(&linter).await;

    // Then
    assert_eq!(
        explorer_clone, linter_clone,
        "two agents on one host must share one clone"
    );
}

/// Two daemons get one clone each — sharing across hosts is not even expressible, but the entry
/// must record the *right* one, or a tool call reads another host's tree.
#[tokio::test]
#[serial]
async fn gives_each_owning_daemon_its_own_clone() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(
        &[(DAEMON_B, &["explorer"]), (DAEMON_C, &["linter"])],
        model.base_url(),
    )
    .await;
    let explorer = format!("explorer@{DAEMON_B}");
    let linter = format!("linter@{DAEMON_C}");

    // When
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.attach(&linter).await.expect("attach linter");
    let explorer_clone = fleet.await_clone_ready(&explorer).await;
    let linter_clone = fleet.await_clone_ready(&linter).await;

    // Then
    assert_ne!(
        explorer_clone, linter_clone,
        "agents on different hosts must not share a clone id"
    );
    assert!(
        peer_session_ids(&fleet, DAEMON_B)
            .await
            .contains(&explorer_clone),
        "daemon B must hold explorer's clone"
    );
    assert!(
        peer_session_ids(&fleet, DAEMON_C)
            .await
            .contains(&linter_clone),
        "daemon C must hold linter's clone"
    );
}

// ---------------------------------------------------------------------------
// AC31-AC32 — reads local, writes proxied
// ---------------------------------------------------------------------------

/// A remote agent reads from its own clone. That is the entire reason for placing an agent on
/// another host: its reads are local and fast, against a real checkout.
///
/// Driven through the agent's own turn loop — the model asks for the file, the loop dispatches the
/// call on the owning daemon — because that is the only caller the split has in production.
#[tokio::test]
#[serial]
async fn reads_a_remote_agents_files_from_its_own_clone() {
    // Given — a file that exists ONLY in the clone, written after it was provisioned
    let model = a_stub_model()
        .calling("READ", serde_json::json!({ "path": "only-in-clone.txt" }))
        .saying("<final_answer>read</final_answer>")
        .start()
        .await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.await_clone_ready(&explorer).await;
    let clone_root = fleet
        .a
        .agent_clone_worktree_path(&fleet.session_id, &explorer)
        .await
        .expect("the roster must know where the clone is");
    std::fs::write(
        clone_root.join("only-in-clone.txt"),
        "read-from-the-clone\n",
    )
    .expect("write a file only the clone has");

    // When — the agent runs the READ its model asked for
    let conversation = fleet.a_conversation_with(&explorer).await;
    collect_prompt(&fleet, &conversation, "what does only-in-clone.txt say?").await;

    // Then — the loop's tool result carries the clone's copy, which only the clone could serve
    let tool_results = model.tool_results();
    assert_eq!(
        tool_results.len(),
        1,
        "the loop must have run exactly the one READ the model asked for, saw {tool_results:?}"
    );
    // Containment, not equality: the tool engine wraps the file in its own read envelope
    // (line window, path, byte counts) and only the content is this test's business.
    assert!(
        tool_results[0].contains("read-from-the-clone"),
        "the read must be answered from the clone, was: {}",
        tool_results[0]
    );
    assert!(
        !fleet.authoritative_worktree().join("only-in-clone.txt").exists(),
        "the fixture must not have written into the authoritative worktree, or the test proves nothing"
    );
}

/// A mutation lands in the facilitating daemon's worktree. The clone is a mirror of that worktree:
/// a write applied there would be overwritten by the next sync tick and would never reach the
/// session's branch — and since the mirror only ever flows A → clone, a file present in A's tree is
/// one the clone did not keep to itself.
#[tokio::test]
#[serial]
async fn lands_a_remote_agents_write_in_the_authoritative_worktree() {
    // Given
    let model = a_stub_model()
        .calling(
            "WRITE",
            serde_json::json!({
                "path": "written-by-agent.txt",
                "contents": "written by a remote agent\n",
            }),
        )
        .saying("<final_answer>written</final_answer>")
        .start()
        .await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.await_clone_ready(&explorer).await;

    // When — the agent runs the WRITE its model asked for
    let conversation = fleet.a_conversation_with(&explorer).await;
    collect_prompt(&fleet, &conversation, "write the note").await;

    // Then
    let landed = fleet.authoritative_worktree().join("written-by-agent.txt");
    let contents = std::fs::read_to_string(&landed).unwrap_or_else(|e| {
        panic!(
            "a remote agent's write must land in the facilitating daemon's worktree ({landed:?}): \
             {e}; the loop's tool results were {:?}",
            model.tool_results()
        )
    });
    assert_eq!(contents, "written by a remote agent\n");
}

/// The clone's link to the facilitating daemon settles *which* tree a tool works, never *who* may
/// work it. A daemon holding a clone answers exec tools for a session that is not its own, and the
/// mutating half proxies them under the clone's own stored credential — so a caller that presented
/// no credential at all would be landing an arbitrary write in another host's authoritative worktree
/// under a token it never held. The session id it would need is published in `session.agents`.
#[tokio::test]
#[serial]
async fn refuses_an_unauthenticated_tool_call_against_a_clone_it_hosts() {
    // Given — a ready clone on daemon B, for a session facilitated by daemon A
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.await_clone_ready(&explorer).await;

    // When — a participant that can address daemon B names that session with no session token
    let result = fleet
        .peer(DAEMON_B)
        .service
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: String::new(),
            session_id: fleet.session_id.clone(),
            daemon_instance_id: String::new(),
            tool_name: "Write".to_string(),
            args_json: serde_json::json!({
                "path": "smuggled-by-an-unauthenticated-caller.txt",
                "content": "smuggled\n",
            })
            .to_string(),
        }))
        .await
        .map(|r| r.into_inner());

    // Then
    let status = result.expect_err("an unauthenticated tool call must be refused");
    assert_eq!(status.code(), Code::Unauthenticated);
    assert!(
        !fleet
            .authoritative_worktree()
            .join("smuggled-by-an-unauthenticated-caller.txt")
            .exists(),
        "a refused tool call must not have reached the facilitating daemon's worktree"
    );
}

// ---------------------------------------------------------------------------
// AC33-AC35 — failure is visible
// ---------------------------------------------------------------------------

/// A prompt sent before the clone exists is refused naming the state. Queuing it would make a
/// 90-second `git clone` look like a hung agent; serving it would read an empty checkout and report
/// "not found" for a file that is simply not there yet.
#[tokio::test]
#[serial]
async fn refuses_a_prompt_while_the_clone_is_still_being_built() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    let roster = fleet.attach(&explorer).await.expect("attach explorer");

    // When — immediately, before awaiting readiness
    let entry_state = roster.entry(&explorer).clone_state;
    let result = fleet
        .a
        .open_agent_conversation(Request::new(OpenAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: fleet.session_id.clone(),
            daemon_instance_id: String::new(),
            agent_id: explorer.clone(),
            conversation_id: String::new(),
        }))
        .await;

    // Then — either the clone was already ready (nothing to assert) or the refusal names the state
    assert_eq!(
        entry_state, CLONE_STATE_PROVISIONING,
        "the fixture must observe the provisioning window for this test to mean anything"
    );
    let status = result.expect_err("a prompt during provisioning must be refused");
    assert_eq!(status.code(), Code::FailedPrecondition);
    assert!(
        status.message().contains("provisioning"),
        "the refusal must name the clone state, was: {}",
        status.message()
    );
}

/// An attach that cannot reach its owning daemon leaves nothing behind — no entry, no revision
/// bump, no half-built clone on a host nobody is watching.
#[tokio::test]
#[serial]
async fn leaves_nothing_behind_when_the_owning_daemon_cannot_be_reached() {
    // Given — a daemon that is not in the room at all
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let absent = "explorer@daemon-that-is-not-here".to_string();

    // When
    let result = fleet.attach(&absent).await;

    // Then
    let status = result.expect_err("attaching to an unreachable daemon must fail");
    assert!(
        matches!(status.code(), Code::InvalidArgument | Code::Unavailable),
        "unexpected code {:?}: {}",
        status.code(),
        status.message()
    );
    let roster = fleet.roster().await;
    assert!(
        roster.agents.is_empty(),
        "no entry may survive a failed attach"
    );
    assert_eq!(
        roster.rev, 0,
        "a failed attach must not advance the revision"
    );
}

/// One daemon going away fails only its own agents. The roster is not all-or-nothing: a session
/// with agents on three hosts keeps working when one is rebooted.
#[tokio::test]
#[serial]
async fn fails_only_the_agents_of_a_daemon_that_goes_away() {
    // Given
    let model = a_stub_model()
        .saying("<final_answer>still here</final_answer>")
        .start()
        .await;
    let fleet = a_fleet_with_peers(
        &[(DAEMON_B, &["explorer"]), (DAEMON_C, &["linter"])],
        model.base_url(),
    )
    .await;
    let explorer = format!("explorer@{DAEMON_B}");
    let linter = format!("linter@{DAEMON_C}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.attach(&linter).await.expect("attach linter");
    fleet.await_clone_ready(&explorer).await;
    fleet.await_clone_ready(&linter).await;

    // When — daemon C leaves the room
    fleet.peer(DAEMON_C).run.abort();

    // Then — B's agent still answers
    let conversation = fleet
        .a
        .open_agent_conversation(Request::new(OpenAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: fleet.session_id.clone(),
            daemon_instance_id: String::new(),
            agent_id: explorer.clone(),
            conversation_id: String::new(),
        }))
        .await
        .expect("the surviving daemon's agent must still be reachable")
        .into_inner()
        .conversation_id;
    let answer = collect_prompt(&fleet, &conversation, "are you there?").await;
    assert!(answer.content.contains("still here"));

    // and C's agent fails naming the daemon
    let status = fleet
        .a
        .open_agent_conversation(Request::new(OpenAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: fleet.session_id.clone(),
            daemon_instance_id: String::new(),
            agent_id: linter.clone(),
            conversation_id: String::new(),
        }))
        .await
        .expect_err("an agent on a departed daemon must fail");
    assert!(
        status.message().contains(DAEMON_C),
        "the failure must name the daemon that went away, was: {}",
        status.message()
    );
}

// ---------------------------------------------------------------------------
// AC36-AC40 — the clone is a real, synced checkout
// ---------------------------------------------------------------------------

/// The clone is its own checkout — not the project directory, and not a worktree the facilitating
/// daemon owns. Reusing either would put a second agent's tools in the operator's own tree.
#[tokio::test]
#[serial]
async fn clones_into_a_checkout_that_is_neither_the_project_nor_a_worktree_in_use() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.await_clone_ready(&explorer).await;

    // When
    let clone_root = fleet
        .a
        .agent_clone_worktree_path(&fleet.session_id, &explorer)
        .await
        .expect("the roster must know where the clone is");

    // Then
    assert_ne!(
        clone_root,
        fleet.authoritative_worktree(),
        "the clone must not be the session's own worktree"
    );
    assert!(
        clone_root.starts_with(fleet.peer(DAEMON_B).sessions.path()),
        "the clone must live under the owning daemon's sessions base, was {clone_root:?}"
    );
    assert!(
        clone_root.join("README.md").exists(),
        "the clone must be a real checkout of the project"
    );
}

/// The clone follows the session's uncommitted work. Without this a remote reviewer reads the last
/// commit and reports on code the agent has already replaced.
#[tokio::test]
#[serial]
async fn shows_a_remote_agent_work_the_main_agent_has_not_committed() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.await_clone_ready(&explorer).await;
    let clone_root = fleet
        .a
        .agent_clone_worktree_path(&fleet.session_id, &explorer)
        .await
        .expect("clone path");

    // When — an uncommitted edit in the session's worktree
    std::fs::write(
        fleet.authoritative_worktree().join("src/main.rs"),
        "fn main() { println!(\"edited\"); }\n",
    )
    .expect("edit the session worktree");

    // Then
    // 30s: bounded by the room's poll interval plus one delta application.
    let mirrored = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let contents =
                std::fs::read_to_string(clone_root.join("src/main.rs")).unwrap_or_default();
            if contents.contains("edited") {
                return contents;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .expect("the uncommitted edit never reached the clone");
    assert!(mirrored.contains("edited"));
}

/// A commit in the session moves the clone's HEAD to the same sha, so a remote agent's view of
/// history matches the main agent's.
#[tokio::test]
#[serial]
async fn moves_the_clone_to_the_commit_the_session_made() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.await_clone_ready(&explorer).await;
    let clone_root = fleet
        .a
        .agent_clone_worktree_path(&fleet.session_id, &explorer)
        .await
        .expect("clone path");

    // When
    let worktree = fleet.authoritative_worktree();
    std::fs::write(worktree.join("NOTES.md"), "committed\n").expect("write");
    git(&worktree, &["add", "-A"]);
    git(&worktree, &["commit", "-q", "-m", "second"]);
    let expected_head = git(&worktree, &["rev-parse", "HEAD"]).trim().to_string();

    // Then
    // 30s: one poll tick plus a fetch of the session's ref.
    let head = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let head = git(&clone_root, &["rev-parse", "HEAD"]).trim().to_string();
            if head == expected_head {
                return head;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .expect("the clone never reached the session's commit");
    assert_eq!(head, expected_head);
}

/// A clone corrupted by hand is restored, and the divergence is reported rather than absorbed
/// silently — a mirror that repairs itself without saying so hides a real fault.
#[tokio::test]
#[serial]
async fn restores_a_clone_that_diverged_and_says_so() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.await_clone_ready(&explorer).await;
    let clone_root = fleet
        .a
        .agent_clone_worktree_path(&fleet.session_id, &explorer)
        .await
        .expect("clone path");

    // When — the clone is edited underneath the syncer, then the session moves
    std::fs::write(clone_root.join("README.md"), "corrupted by hand\n").expect("corrupt clone");
    std::fs::write(
        fleet.authoritative_worktree().join("README.md"),
        "# agent roster fixture v2\n",
    )
    .expect("edit the session worktree");

    // Then
    // 30s: one poll tick, a rejected patch, and a reconcile.
    let restored = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let contents =
                std::fs::read_to_string(clone_root.join("README.md")).unwrap_or_default();
            if contents.contains("fixture v2") {
                return contents;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .expect("the corrupted clone was never restored");
    assert!(restored.contains("fixture v2"));

    let divergences = fleet
        .a
        .agent_clone_divergences(&fleet.session_id, &explorer)
        .await
        .expect("the daemon must record what diverged");
    assert!(
        !divergences.is_empty(),
        "a reconcile must be reported, never silent"
    );
}

// ---------------------------------------------------------------------------
// AC41-AC43 — teardown
// ---------------------------------------------------------------------------

/// Detaching the last agent of a host removes that host's checkout. Leaving it would accumulate a
/// worktree per session per host, on machines the operator is not watching.
#[tokio::test]
#[serial]
async fn removes_the_clone_when_the_last_agent_on_that_host_is_detached() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer", "linter"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    let linter = format!("linter@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.attach(&linter).await.expect("attach linter");
    let clone_session = fleet.await_clone_ready(&explorer).await;

    // When — the first detach leaves the clone, the second removes it
    fleet.detach(&explorer).await.expect("detach explorer");
    assert!(
        peer_session_ids(&fleet, DAEMON_B)
            .await
            .contains(&clone_session),
        "the clone must survive while another agent on that host still uses it"
    );
    fleet.detach(&linter).await.expect("detach linter");

    // Then
    assert!(
        !peer_session_ids(&fleet, DAEMON_B)
            .await
            .contains(&clone_session),
        "the clone must be removed with the last agent that used it"
    );
}

/// A peer that already removed the clone answers "no such session", and that is success — the
/// clone is an ordinary listable session an operator may have deleted directly, and treating it as
/// an error would make the agent permanently undetachable.
#[tokio::test]
#[serial]
async fn treats_a_clone_the_peer_already_removed_as_removed() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach explorer");
    let clone_session = fleet.await_clone_ready(&explorer).await;

    // When — the operator deletes the clone session on B directly, then detaches
    let peer_sessions_root = fleet.peer(DAEMON_B).sessions.path().join("sessions");
    std::fs::remove_dir_all(peer_sessions_root.join(&clone_session))
        .expect("remove the clone session on the peer");

    // Then
    let roster = fleet
        .detach(&explorer)
        .await
        .expect("a clone the peer no longer has is already torn down, not an error");
    assert!(roster.agents.is_empty());
}

/// Deleting the session removes every clone it created — including on hosts the operator never
/// looked at.
#[tokio::test]
#[serial]
async fn removes_every_clone_when_the_session_is_deleted() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(
        &[(DAEMON_B, &["explorer"]), (DAEMON_C, &["linter"])],
        model.base_url(),
    )
    .await;
    let explorer = format!("explorer@{DAEMON_B}");
    let linter = format!("linter@{DAEMON_C}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.attach(&linter).await.expect("attach linter");
    let clone_b = fleet.await_clone_ready(&explorer).await;
    let clone_c = fleet.await_clone_ready(&linter).await;

    // When
    fleet
        .a
        .delete_session(Request::new(DeleteSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: fleet.session_id.clone(),
        }))
        .await
        .expect("deleting the session must succeed");

    // Then
    assert!(!peer_session_ids(&fleet, DAEMON_B).await.contains(&clone_b));
    assert!(!peer_session_ids(&fleet, DAEMON_C).await.contains(&clone_c));
    let remaining = fleet
        .a
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: TEST_TOKEN.to_string(),
        }))
        .await
        .expect("listing sessions must succeed")
        .into_inner()
        .sessions;
    assert!(
        !remaining.iter().any(|s| s.session_id == fleet.session_id),
        "the session itself must be gone"
    );
}

/// A peer that provisioned the project by clone reports it — the checkout arrived on a host that
/// had never seen the repository, which is what makes attaching to a fresh workstation work.
#[tokio::test]
#[serial]
async fn provisions_the_project_on_a_daemon_that_has_never_seen_it() {
    // Given — daemon B's registry deliberately does not list the project
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let peer_projects = fleet.peer(DAEMON_B).sessions.path().join("projects");
    std::fs::remove_dir_all(&peer_projects).expect("remove the peer's project registry");
    let explorer = format!("explorer@{DAEMON_B}");

    // When
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.await_clone_ready(&explorer).await;

    // Then
    let clone_root = fleet
        .a
        .agent_clone_worktree_path(&fleet.session_id, &explorer)
        .await
        .expect("clone path");
    assert!(
        clone_root.join("README.md").exists(),
        "the peer must have provisioned the project it did not have"
    );
}

/// Sanity guard on the fixture itself: an `ExecuteTool` addressed at the facilitating daemon still
/// answers from the authoritative worktree, so the read/write split above is a real split and not
/// an artefact of everything landing in one place.
#[tokio::test]
#[serial]
async fn keeps_the_facilitating_daemon_as_the_identity_file_reads_are_addressed_to() {
    // Given
    let model = a_stub_model().saying("ok").start().await;
    let fleet = a_fleet_with_peers(&[(DAEMON_B, &["explorer"])], model.base_url()).await;
    let explorer = format!("explorer@{DAEMON_B}");
    fleet.attach(&explorer).await.expect("attach explorer");
    fleet.await_clone_ready(&explorer).await;

    // When
    let result = fleet
        .a
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: fleet.session_id.clone(),
            daemon_instance_id: String::new(),
            tool_name: "Read".to_string(),
            args_json: serde_json::json!({ "path": "README.md" }).to_string(),
        }))
        .await
        .expect("the facilitating daemon must still serve its own worktree")
        .into_inner();

    // Then
    assert!(
        result.result_json.contains("agent roster fixture"),
        "the read must come from the authoritative worktree, was: {}",
        result.result_json
    );
}
