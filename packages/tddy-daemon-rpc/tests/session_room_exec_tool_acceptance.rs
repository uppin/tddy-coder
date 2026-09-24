//! Acceptance: a participant in a session's LiveKit room reaches the worktree through the exec
//! tools the facilitating daemon serves there.
//!
//! Product contract: `docs/ft/daemon/session-room.md` (AC3)
//!
//! Split from `tddy-session-lifecycle`'s `session_room_acceptance.rs`, which pins the room itself.
//! The exec tools are served by `tddy-daemon-rpc`'s `ExecToolRpcHandler` and reach a room through
//! the host's `DaemonRpcFamilies` port, so the one test that calls a tool through a room lives here,
//! where the handlers `runtime::build` installs are available. The fixture is that suite's, trimmed
//! to what this test reaches.
//!
//! Needs the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`), and is `#[serial]` so
//! it owns it alone.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::RoomOptions;
use livekit::Room;
use prost::Message;
use serial_test::serial;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::session_room::session_room_name;
use tddy_livekit::{LiveKitRpcClientFactory, RpcClient};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{
    ConnectSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
    StartSessionResponse,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::stub_scripts::a_stub_agent_script;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The lobby, so the daemon under test is configured exactly as in production: the session room is
/// an addition to it, not a replacement.
const COMMON_ROOM: &str = "session-room-lobby";
const INSTANCE_ID: &str = "session-room-facilitating-host";
const LK_API_KEY: &str = "devkey";
const LK_API_SECRET: &str = "secret";
const TEST_PROJECT_ID: &str = "session-room-proj";

/// Short enough that a change is picked up inside a test's patience, long enough that a loaded
/// machine is not spending its whole slice shelling out to git.
const POLL_INTERVAL_MS: u64 = 200;

/// A file committed into the repository before any worktree is cut from it, so every checkout has a
/// tracked file from the moment its room opens.
const SEEDED_FILE: &str = "seeded.txt";
const SEEDED_CONTENTS: &str = "one\ntwo\n";

const CALL_TIMEOUT: Duration = Duration::from_secs(20);

/// The identity a daemon serves its RPC surface on. Fixed `daemon-` prefix, not a lookup — see
/// `docs/ft/web/daemon-selector-livekit-rpc.md`.
fn rpc_identity(instance_id: &str) -> String {
    format!("daemon-{instance_id}")
}

fn true_bin() -> String {
    ["/bin/true", "/usr/bin/true"]
        .iter()
        .find(|p| Path::new(p).exists())
        .expect("a `true` binary must exist for the tool allowlist")
        .to_string()
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Runs `git` in `dir` with a fixed identity, so a machine with no git config still commits.
fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .expect("git must be on PATH");
    assert!(
        output.status.success(),
        "git {args:?} failed in {dir:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The sha `HEAD` resolves to in `dir`.
///
/// Checked rather than trusted, because the daemon shares this helper's failure mode: a snapshot
/// whose git could not run reports `head_commit: ""`, so an unchecked helper would hand back `""`
/// too and an assertion would compare one failure against another.
fn head_commit_of(dir: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("git must be on PATH");
    assert!(
        output.status.success(),
        "git rev-parse HEAD failed in {dir:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // The sha is whatever git minted at commit time — only its shape can be pinned here.
    assert!(
        sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "git rev-parse HEAD in {dir:?} answered {sha:?}, which is not a commit sha"
    );
    sha
}

/// A repo whose `origin` points at itself, so worktree setup's `git fetch origin` succeeds with no
/// server, holding [`SEEDED_FILE`] from its first commit.
fn create_test_repo_with_origin(dir: &Path) {
    git(dir, &["init", "-b", "main"]);
    git(dir, &["config", "user.email", "t@t.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join(SEEDED_FILE), SEEDED_CONTENTS).expect("seed the repository");
    git(dir, &["add", SEEDED_FILE]);
    git(dir, &["commit", "-m", "seed"]);
    git(dir, &["remote", "add", "origin", dir.to_str().unwrap()]);
    git(dir, &["push", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: session-room-proj\n    git_url: \"\"\n    main_repo_path: {}\n",
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// The `livekit:` block a daemon needs to host session rooms.
fn livekit_yaml_block(ws_url: &str) -> String {
    format!(
        "livekit:\n  enabled: true\n  url: {ws_url}\n  api_key: {LK_API_KEY}\n  \
         api_secret: {LK_API_SECRET}\n  common_room: {COMMON_ROOM}\n"
    )
}

fn write_daemon_yaml(
    ws_url: &str,
    os_user: &str,
    claude_binary: &Path,
) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    let true_path = true_bin();
    let livekit = livekit_yaml_block(ws_url);
    let claude_binary = claude_binary.display();
    let yaml = format!(
        r#"
daemon_instance_id: {INSTANCE_ID}
users:
  - github_user: "testuser"
    os_user: "{os_user}"
allowed_tools:
  - path: {true_path}
    label: t
claude_cli:
  binary_path: {claude_binary}
session_room:
  poll_interval_ms: {POLL_INTERVAL_MS}
{livekit}"#
    );
    std::fs::write(&path, yaml).unwrap();
    (dir, path)
}

/// A session that runs an agent, and therefore has a room.
fn an_agent_session_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-5".to_string(),
        ..Default::default()
    }
}

/// The daemon running the agent: it hosts the session room and answers tool calls in it.
struct FacilitatingDaemon {
    service: DaemonSessionHost,
    sessions_base: PathBuf,
    ws_url: String,
    livekit: LiveKitTestkit,
    _sessions: tempfile::TempDir,
    _staging: tempfile::TempDir,
    _config: tempfile::TempDir,
    _repo: tempfile::TempDir,
    _stubs: tempfile::TempDir,
}

impl FacilitatingDaemon {
    /// A daemon with the families `runtime::build` installs, so the room's roster is the
    /// production one — the exec tools among them.
    async fn with_livekit() -> Self {
        let livekit = LiveKitTestkit::start()
            .await
            .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
        let ws_url = livekit.get_ws_url();

        let os_user = std::env::var("USER").expect("USER required");
        let repo_dir = tempfile::tempdir().unwrap();
        create_test_repo_with_origin(repo_dir.path());

        // Holds its PTY open the way a real agent waiting for a turn does: a room belongs to a
        // session that runs an agent, and without a stub the start would reach for `claude` on PATH.
        let stub_dir = tempfile::tempdir().unwrap();
        let claude_stub = a_stub_agent_script(stub_dir.path(), "stub-claude.sh")
            .echoing_argv()
            .then_reading_stdin()
            .build();

        let (config_dir, config_path) = write_daemon_yaml(&ws_url, &os_user, &claude_stub);
        let config = DaemonConfig::load(&config_path).expect("daemon.yaml must load");

        let sessions = tempfile::tempdir().unwrap();
        register_project(&sessions.path().join("projects"), repo_dir.path());
        let base = sessions.path().to_path_buf();
        let resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        let user_resolver: UserResolver =
            Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));

        let staging = tempfile::tempdir().unwrap();
        let service = DaemonSessionHost::new(
            config,
            resolver,
            sessions.path().to_path_buf(),
            user_resolver,
            None,
            None,
            None,
            Arc::new(tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager::new()),
        )
        .with_staging_base_dir(staging.path().to_path_buf());
        let (service, _) = tddy_daemon_rpc::RpcHandlers::install(service);

        Self {
            service,
            sessions_base: sessions.path().to_path_buf(),
            ws_url,
            livekit,
            _sessions: sessions,
            _staging: staging,
            _config: config_dir,
            _repo: repo_dir,
            _stubs: stub_dir,
        }
    }

    /// A started agent session that something has connected to, so its room is open.
    async fn a_session_being_connected_to(&self) -> StartSessionResponse {
        let started = self
            .service
            .start_session(Request::direct(an_agent_session_request()))
            .await
            .expect("an agent session must start")
            .into_inner();
        self.service
            .connect_session(Request::direct(ConnectSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: started.session_id.clone(),
            }))
            .await
            .expect("connecting to a started session must open its room");
        started
    }

    fn worktree_of(&self, session_id: &str) -> PathBuf {
        let dir = unified_session_dir_path(&self.sessions_base, session_id);
        let metadata = tddy_core::read_session_metadata(&dir)
            .unwrap_or_else(|e| panic!("session metadata for {session_id} must be readable: {e}"));
        PathBuf::from(
            metadata
                .repo_path
                .expect("a workspace session must record its worktree"),
        )
    }
}

/// A participant that joins the session room the way another agent would: no service of its own,
/// just an RPC client aimed at the facilitating daemon.
struct AgentProbe {
    room: Arc<Room>,
}

impl AgentProbe {
    async fn join(daemon: &FacilitatingDaemon, room_name: &str, identity: &str) -> Self {
        let token = daemon
            .livekit
            .generate_token(room_name, identity)
            .expect("LiveKit token for an agent probe");
        let (room, _events) = Room::connect(&daemon.ws_url, &token, RoomOptions::default())
            .await
            .unwrap_or_else(|e| panic!("probe {identity} must join {room_name}: {e}"));
        Self {
            room: Arc::new(room),
        }
    }

    fn rpc_to_daemon(&self) -> RpcClient {
        LiveKitRpcClientFactory::for_room(self.room.clone()).client(rpc_identity(INSTANCE_ID))
    }
}

async fn execute_tool_in_room(
    client: &RpcClient,
    session_id: &str,
    tool_name: &str,
    args: serde_json::Value,
) -> ExecuteToolResponse {
    let bytes = tokio::time::timeout(
        CALL_TIMEOUT,
        client.call_unary(
            "exec_tools.ExecToolService",
            "ExecuteTool",
            ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                tool_name: tool_name.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: INSTANCE_ID.to_string(),
            }
            .encode_to_vec(),
        ),
    )
    .await
    .expect("ExecuteTool in the session room must return within the timeout")
    .expect("ExecuteTool in the session room must succeed");
    ExecuteToolResponse::decode(&bytes[..]).expect("ExecuteToolResponse must decode")
}

/// Commits `contents` at `path` in the worktree and yields the resulting HEAD sha.
fn commit_a_file(worktree: &Path, path: &str, contents: &str) -> String {
    std::fs::write(worktree.join(path), contents).expect("write into the worktree");
    git(worktree, &["add", path]);
    git(worktree, &["commit", "-m", &format!("add {path}")]);
    head_commit_of(worktree)
}

// ---------------------------------------------------------------------------
// AC3 — file access is served in the room
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_participant_reads_a_worktree_file_through_execute_tool_in_the_session_room() {
    // Given a worktree holding a committed file, and an agent that joined only the session room
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon.a_session_being_connected_to().await;
    let worktree = daemon.worktree_of(&started.session_id);
    commit_a_file(&worktree, "greeting.txt", "hello from the worktree");

    let probe = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-file-reader",
    )
    .await;

    // When it reads that file through the daemon's RPC surface in this room
    let response = execute_tool_in_room(
        &probe.rpc_to_daemon(),
        &started.session_id,
        "Read",
        serde_json::json!({ "path": "greeting.txt" }),
    )
    .await;

    // Then it gets the checkout's contents — file access is served where the worktree lives, by the
    // daemon that owns it, without the caller ever joining the lobby
    assert!(
        !response.is_error,
        "the tool call must succeed; error was '{}'",
        response.error_message
    );
    let result: serde_json::Value =
        serde_json::from_str(&response.result_json).expect("result_json must be JSON");
    assert_eq!(result["content"], "hello from the worktree");
}
