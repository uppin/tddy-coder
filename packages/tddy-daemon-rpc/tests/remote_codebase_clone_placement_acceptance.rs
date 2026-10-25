//! Acceptance: remote placement on a remote-codebase session.
//!
//! `clone` placement on a remote-codebase session is the per-conversation worktree that the codebase
//! daemon serves through `open_local`, and the relay carries the conversation's worktree operations
//! to that daemon. These tests pin three things the feature needs:
//!
//! - a clone-placed coding agent of a `workspace` session is launched on its conversation's worktree,
//!   under the session's own worktree, naming the conversation;
//! - the relay's `ConversationWorktree` HTTP arm (`tddy-session-tool-client` over the daemon's `/rpc`)
//!   answers a conversation's diff from the codebase host it forwards to;
//! - a pull of that conversation, asked through the relay, applies its change to the codebase host's
//!   session worktree.
//!
//! The relay and the codebase host are two in-process daemons. The common room between them is an
//! in-process forwarder that hands each forwarded call to the codebase daemon's own exec-tool
//! service, so the routing that decides the hop is the production code. The LiveKit transport is not
//! run here: the two-daemon LiveKit suite is not part of this change's local run.
//!
//! Feature: docs/ft/daemon/remote-codebase-mode.md § Coding subagents on the codebase host ("Where the agent runs").

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use prost::Message as _;
use serde_json::json;
use serial_test::serial;
use tddy_coding_subagent::{CodingAgentLaunch, CodingAgentLauncher};
use tddy_connectrpc::connect_router;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{AcpAgentCommand, McpServerSpec, SessionMetadata};
use tddy_daemon_extensions::PeerForwarder;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::operator_channel::NoOperatorChannel;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_daemon_rpc::test_util::{test_service_with_coding_agent_launcher, TestDaemon};
use tddy_discovery::subagent::SubagentError;
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_rpc::{MultiRpcService, Request, RpcBridge, Status};
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::exec_tools::{
    ConversationWorktreeRequest, ConversationWorktreeResponse, ExecToolService as _,
    ExecuteToolRequest,
};
use tddy_service::proto::session_agents_svc::{
    AgentPlacement, AttachSessionAgentRequest, OpenAgentConversationRequest,
    SessionAgentService as _,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::test_util::{TEST_TOKEN, TEST_USER};
use tddy_session_tool_client::{
    conversation_worktree, diff_conversation_worktree, ConversationWorktreeOp,
};
use tddy_tool_engine::EXEC_TOOL_SERVICE;

const SESSION_ID: &str = "1780828020298-remote-placement";
const CONVERSATION: &str = "explore";
const CODEBASE: &str = "codebase-host";
const RELAY: &str = "relay-host";
const CODEX: &str = "codex-fast";
const WRITTEN: &str = "src/new.rs";
const WRITTEN_CONTENTS: &str = "pub fn new() {}\n";

// ---------------------------------------------------------------------------
// Fixtures shared by the three tests
// ---------------------------------------------------------------------------

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A git checkout with one commit, the worktree a remote-codebase session works on.
fn a_git_worktree(dir: &Path) {
    git(dir, &["init", "-q", "-b", "master"]);
    git(dir, &["config", "user.email", "developer@example.com"]);
    git(dir, &["config", "user.name", "Developer"]);
    std::fs::write(dir.join("README.md"), "hello\n").expect("seed file");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

/// A `workspace` session, the type a remote-codebase session is, on `worktree`. Not managed: the
/// codebase host runs its agents there as the session's own user, without a managed confinement.
fn write_workspace_session(sessions_base: &Path, worktree: &Path) {
    let session_dir = unified_session_dir_path(sessions_base, SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let mut metadata: SessionMetadata = serde_json::from_value(json!({
        "session_id": SESSION_ID,
        "project_id": "project-remote-placement",
        "created_at": "2026-10-25T10:00:00Z",
        "updated_at": "2026-10-25T10:00:00Z",
        "status": "active",
        "session_type": "workspace",
    }))
    .expect("metadata");
    metadata.repo_path = Some(worktree.display().to_string());
    tddy_core::write_session_metadata(&session_dir, &metadata).expect("write metadata");
}

/// A daemon named `instance_id` whose sessions live under `sessions_base`, routing by `peers`.
///
/// The daemon's own configured id is what the forwarded request is classified against, so the
/// codebase host must be built under the id the relay addresses it by.
fn a_daemon(
    instance_id: &str,
    sessions_base: &Path,
    peers: impl FnOnce(&DaemonConfig) -> PeerRouting,
) -> DaemonSessionHost {
    let config_path = sessions_base.join("config.yaml");
    std::fs::write(
        &config_path,
        format!(
            "daemon_instance_id: \"{instance_id}\"\nusers:\n  - github_user: \"testuser\"\n    \
             os_user: \"testdev\"\n"
        ),
    )
    .expect("write daemon config");
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    let routing = peers(&config);
    let base = sessions_base.to_path_buf();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| TEST_USER.to_string()));
    DaemonSessionHost::new(
        config,
        sessions_base_resolver,
        sessions_base.to_path_buf(),
        user_resolver,
        None,
        routing,
        Arc::new(NoOperatorChannel),
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

/// The codebase host's session worktree holds a conversation's committed change, written through its
/// own `ExecuteTool` with the conversation named, as a subagent's `Write` is.
struct ACodebaseHost {
    daemon: TestDaemon,
    worktree: PathBuf,
    _sessions: tempfile::TempDir,
    _worktree: tempfile::TempDir,
}

async fn a_codebase_host_with_a_conversation_change() -> ACodebaseHost {
    let sessions = tempfile::tempdir().expect("sessions dir");
    let worktree_dir = tempfile::tempdir().expect("worktree dir");
    let worktree = worktree_dir.path().to_path_buf();
    a_git_worktree(&worktree);
    write_workspace_session(sessions.path(), &worktree);
    let host = a_daemon(CODEBASE, sessions.path(), |config| {
        PeerRouting::local_only(config)
    });
    let daemon = TestDaemon::from_host(host);
    daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            tool_name: "Write".to_string(),
            args_json: json!({ "path": WRITTEN, "contents": WRITTEN_CONTENTS }).to_string(),
            daemon_instance_id: String::new(),
            conversation_id: CONVERSATION.to_string(),
        }))
        .await
        .expect("the conversation's Write answers");
    ACodebaseHost {
        daemon,
        worktree,
        _sessions: sessions,
        _worktree: worktree_dir,
    }
}

/// Lists the two daemons of the common room: the relay and the codebase host it forwards to.
struct TheTwoDaemons;

#[async_trait::async_trait]
impl EligibleDaemonSource for TheTwoDaemons {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        [RELAY, CODEBASE]
            .into_iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(id.to_string()),
                label: format!("{id} (common room)"),
            })
            .collect()
    }
}

/// The codebase host behind the common room. Each forwarded call is handed to that daemon's own
/// exec-tool service, and every call the room forwards is recorded, so a test can see the hop.
struct TheCodebaseHostBehindTheRoom {
    codebase: TestDaemon,
    carried: Mutex<Vec<(String, String, String)>>,
}

impl TheCodebaseHostBehindTheRoom {
    fn carried(&self) -> Vec<(String, String, String)> {
        self.carried.lock().expect("carried").clone()
    }
}

#[async_trait::async_trait]
impl PeerForwarder for TheCodebaseHostBehindTheRoom {
    async fn is_connected(&self) -> bool {
        true
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        self.carried.lock().expect("carried").push((
            peer_id.to_string(),
            service.to_string(),
            method.to_string(),
        ));
        if (service, method) != (EXEC_TOOL_SERVICE, "ConversationWorktree") {
            return Err(Status::unavailable(format!(
                "{service}/{method} is not answered by the codebase host in this test"
            )));
        }
        let request = ConversationWorktreeRequest::decode(body.as_slice())
            .map_err(|e| Status::internal(format!("decode ConversationWorktreeRequest: {e}")))?;
        let answer: ConversationWorktreeResponse = self
            .codebase
            .conversation_worktree(Request::direct(request))
            .await?
            .into_inner();
        Ok(answer.encode_to_vec())
    }

    async fn call_server_stream(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        _body: Vec<u8>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        self.carried.lock().expect("carried").push((
            peer_id.to_string(),
            service.to_string(),
            method.to_string(),
        ));
        Err(Status::unavailable(format!(
            "{service}/{method} is not answered by the codebase host in this test"
        )))
    }
}

/// The relay in front of the codebase host, and the forwarder that carries its calls there.
struct ARelayInFrontOfTheCodebaseHost {
    relay: TestDaemon,
    hop: Arc<TheCodebaseHostBehindTheRoom>,
    codebase: ACodebaseHost,
}

fn a_relay_in_front_of(codebase: ACodebaseHost) -> ARelayInFrontOfTheCodebaseHost {
    let hop = Arc::new(TheCodebaseHostBehindTheRoom {
        codebase: codebase.daemon.clone(),
        carried: Mutex::new(Vec::new()),
    });
    let relay_sessions = tempfile::tempdir().expect("relay sessions dir");
    let forwarder = Arc::clone(&hop);
    let relay = TestDaemon::from_host(a_daemon(RELAY, relay_sessions.path(), move |config| {
        PeerRouting::new(
            config.clone(),
            Arc::new(TheTwoDaemons),
            Some(CommonRoom::new(forwarder, config)),
        )
    }));
    ARelayInFrontOfTheCodebaseHost {
        relay,
        hop,
        codebase,
    }
}

/// The relay's `/rpc` mount on a loopback port, as a remote-codebase session's relay URL names it.
async fn serve_over_http(relay: &TestDaemon) -> String {
    let router = connect_router(RpcBridge::new(MultiRpcService::new(
        relay.handlers().entries(),
    )));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the relay endpoint");
    let port = listener
        .local_addr()
        .expect("relay endpoint local addr")
        .port();
    tokio::spawn(async move {
        // Not swallowed: a relay that stopped serving would read as a refused call otherwise.
        axum::serve(listener, router)
            .await
            .expect("the relay endpoint served");
    });
    format!("http://127.0.0.1:{port}/rpc")
}

/// Points this process's session tool transport at the relay, as a remote-codebase session's
/// environment does, and clears the variables that would select another transport.
fn a_session_reaching_the_codebase_host_through_the_relay(relay_url: &str) {
    for key in [
        "TDDY_SANDBOX_TOOL_IPC",
        "TDDY_REMOTE_DAEMON_SOCKET",
        "TDDY_REMOTE_LIVEKIT_URL",
        "TDDY_REMOTE_LIVEKIT_ROOM",
        "TDDY_REMOTE_LIVEKIT_TOKEN",
        "TDDY_REMOTE_SERVER_IDENTITY",
        "TDDY_SUBAGENT_ROSTER_STATIC",
    ] {
        std::env::remove_var(key);
    }
    std::env::set_var("TDDY_REMOTE_SESSION_ID", SESSION_ID);
    std::env::set_var("TDDY_REMOTE_SESSION_TOKEN", TEST_TOKEN);
    std::env::set_var("TDDY_REMOTE_DAEMON_INSTANCE_ID", CODEBASE);
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", relay_url);
}

fn is_error(answer: &serde_json::Value) -> bool {
    answer["is_error"].as_bool().unwrap_or(false)
}

// ---------------------------------------------------------------------------
// 1. open_local with clone placement on a remote-codebase session
// ---------------------------------------------------------------------------

/// Records every launch the daemon makes; the agent itself is not run here.
struct ALauncherRecordingTheLaunches {
    launches: Mutex<Vec<CodingAgentLaunch>>,
}

#[async_trait::async_trait]
impl CodingAgentLauncher for ALauncherRecordingTheLaunches {
    async fn command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        self.launches.lock().expect("launches").push(launch.clone());
        Ok(AcpAgentCommand {
            program: stub_agent_path(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: Some(launch.worktree.clone()),
        })
    }

    async fn mcp_servers_for(
        &self,
        _launch: &CodingAgentLaunch,
    ) -> Result<Vec<McpServerSpec>, SubagentError> {
        Ok(Vec::new())
    }
}

#[tokio::test]
#[serial]
async fn a_clone_coding_agent_of_a_remote_codebase_session_opens_on_its_conversation_worktree() {
    // Given
    let sessions = tempfile::tempdir().expect("sessions dir");
    let agents = sessions.path().join("agents");
    std::fs::create_dir_all(&agents).expect("agents dir");
    std::fs::write(
        agents.join(format!("{CODEX}.yaml")),
        format!("name: {CODEX}\nharness: codex-acp\nmodel: stub-model\n"),
    )
    .expect("agent profile");
    let worktree_dir = tempfile::tempdir().expect("worktree dir");
    let worktree = worktree_dir.path().to_path_buf();
    a_git_worktree(&worktree);
    write_workspace_session(sessions.path(), &worktree);
    let launcher = Arc::new(ALauncherRecordingTheLaunches {
        launches: Mutex::new(Vec::new()),
    });
    let service =
        test_service_with_coding_agent_launcher(sessions.path().to_path_buf(), launcher.clone());
    let agent = service
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .expect("ListSubagents")
        .into_inner()
        .subagents
        .into_iter()
        .find(|row| row.name == CODEX)
        .unwrap_or_else(|| panic!("a '{CODEX}' row"))
        .agent_id;
    service
        .session_agents_service()
        .attach_session_agent(Request::direct(AttachSessionAgentRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            agent_id: agent.clone(),
            placement: AgentPlacement::Clone as i32,
        }))
        .await
        .expect("attaching the clone-placed agent");

    // When
    service
        .session_agents_service()
        .open_agent_conversation(Request::direct(OpenAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            agent_id: agent,
            conversation_id: CONVERSATION.to_string(),
            system_prompt: String::new(),
        }))
        .await
        .expect("opening the conversation");

    // Then
    let launch = launcher.launches.lock().expect("launches")[0].clone();
    let expected_root = worktree.join("tmp/subagent-worktrees").join(CONVERSATION);
    assert_eq!(
        (
            launch.conversation_id.as_deref(),
            std::fs::canonicalize(&launch.worktree).expect("the launch's worktree exists"),
        ),
        (
            Some(CONVERSATION),
            std::fs::canonicalize(&expected_root).expect("the conversation worktree exists"),
        ),
    );
}

// ---------------------------------------------------------------------------
// 2. The relay's ConversationWorktree HTTP arm answers from the codebase host
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn a_diff_asked_through_the_relay_is_answered_with_the_conversations_change_from_the_codebase_host(
) {
    // Given
    let fixture = a_relay_in_front_of(a_codebase_host_with_a_conversation_change().await);
    let relay_url = serve_over_http(&fixture.relay).await;
    a_session_reaching_the_codebase_host_through_the_relay(&relay_url);

    // When
    let answer: serde_json::Value =
        serde_json::from_str(&diff_conversation_worktree(CONVERSATION, None, None).await)
            .expect("the answer is JSON");

    // Then
    assert_eq!(
        (
            is_error(&answer),
            answer["diff"]["files"].clone(),
            answer["diff"]["diff"]
                .as_str()
                .map(|text| text.contains("+pub fn new() {}")),
            fixture.hop.carried(),
        ),
        (
            false,
            json!({ "created": 1, "updated": 0, "removed": 0 }),
            Some(true),
            vec![(
                CODEBASE.to_string(),
                EXEC_TOOL_SERVICE.to_string(),
                "ConversationWorktree".to_string(),
            )],
        ),
    );
}

// ---------------------------------------------------------------------------
// 3. A pull asked through the relay applies to the codebase host's session worktree
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn a_pull_asked_through_the_relay_brings_the_conversations_change_into_the_codebase_worktree()
{
    // Given
    let fixture = a_relay_in_front_of(a_codebase_host_with_a_conversation_change().await);
    let relay_url = serve_over_http(&fixture.relay).await;
    a_session_reaching_the_codebase_host_through_the_relay(&relay_url);
    let session_worktree = fixture.codebase.worktree.clone();

    // When
    let answer: serde_json::Value = serde_json::from_str(
        &conversation_worktree(CONVERSATION, ConversationWorktreeOp::Pull).await,
    )
    .expect("the answer is JSON");

    // Then
    assert_eq!(
        (
            is_error(&answer),
            session_worktree.join(WRITTEN).exists(),
            fixture.hop.carried().len(),
        ),
        (false, true, 1),
    );
}
