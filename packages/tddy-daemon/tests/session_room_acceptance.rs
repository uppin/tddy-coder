//! Acceptance: a session that runs an agent gets its own LiveKit room, hosted by the daemon running
//! that agent — its *facilitating* daemon.
//!
//! Product contract: `docs/ft/daemon/session-room.md`
//!
//! The room is `session-{session_id}`. The facilitating daemon creates it and joins it as
//! `daemon-{instance_id}` serving its full RPC surface, *before the agent process is spawned* — which
//! is what makes it provably the first participant. Agents join as peers; worktree activity is
//! broadcast to all of them on the `worktree.activity` topic; the room's *metadata* carries the
//! current working-tree summary so a late joiner needs no replay.
//!
//! A `workspace` session runs no agent, so it has no facilitating daemon and gets no room.
//!
//! The room is opened when something first *connects* to the session, not when the session is
//! created: creating a session is local work over a git checkout, and making it wait on a LiveKit
//! server made it cost whatever reaching that server cost.
//!
//! Most of these need the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`) and are
//! `#[serial]` so they own it alone. The ones under § Session creation is a local operation need no
//! container at all — their whole subject is that nothing is dialled.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::RoomOptions;
use livekit::{Room, RoomEvent};
use prost::Message;
use serial_test::serial;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::session_room::{session_room_name, WORKTREE_ACTIVITY_TOPIC};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_livekit::{LiveKitRpcClientFactory, RpcClient};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    session_attachment::Source as AttachmentSource, ConnectSessionRequest,
    ConnectionService as ConnectionServiceTrait, ExecuteToolRequest, ExecuteToolResponse,
    HostDocumentScope, LiveKitRoomInfo, ReadHostDocumentRequest, ReadHostDocumentResponse,
    SessionAttachment, StagedAttachmentRef, StartSessionRequest, StartSessionResponse,
};
use tddy_service::proto::worktree_activity::{WorktreeActivityEvent, WorktreeActivityKind};
use tddy_testing_commons::stub_scripts::a_stub_agent_script;
use tddy_testing_commons::wait::eventually_awaiting;

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

/// The staging batch a fixture's attachments are uploaded under.
const STAGING_ID: &str = "session-room-staging-batch";

/// A file committed into the repository before any worktree is cut from it, so every checkout has a
/// tracked file from the moment its room opens.
///
/// A test that wanted to modify a tracked file would otherwise have to commit one first, and a
/// commit is not one observable transition: `git diff --numstat HEAD` reports a *staged* file, so a
/// poll landing between `git add` and `git commit` announces the staging as a files-changed event of
/// its own. Seeding the file removes the need for that setup entirely.
const SEEDED_FILE: &str = "seeded.txt";
const SEEDED_CONTENTS: &str = "one\ntwo\n";

/// A cold LiveKit container has to accept every participant before any of this can happen, and the
/// daemon shells out to git on a blocking pool for each poll. Sized for the worst machine that will
/// run it; the polling decides when to stop.
/// The allow-list a Claude session syncs, spelled out here so this suite stays independent of
/// tddy-core's per-backend table.
const CLAUDE_CONTEXT_GLOBS: &[&str] = &[
    "CLAUDE.md",
    "AGENTS.md",
    ".claude/**",
    ".mcp.json",
    ".agents/**",
];

/// The `max_attachment_bytes` a co-located context read is bounded by. Named rather than compiled
/// into the source, because it is an operator setting on both halves of a split session.
const A_GENEROUS_CONTEXT_CAP: u64 = 64 * 1024 * 1024;

/// A codebase whose repository carries no agent guidance at all.
///
/// The wiring these tests exercise is the LiveKit room and the token it mints; the context sync
/// that rides along with it is another suite's subject (`context_sync_acceptance.rs`). A local
/// source over an empty repo is the honest stand-in: the fetch really runs and really succeeds,
/// and it has nothing to copy.
fn a_codebase_holding_no_agent_guidance() -> (
    tempfile::TempDir,
    tddy_daemon::context_sync::LocalWorktreeSource,
) {
    let repo = tempfile::tempdir().expect("tempdir");
    let source = tddy_daemon::context_sync::LocalWorktreeSource::new(
        repo.path().to_path_buf(),
        CLAUDE_CONTEXT_GLOBS,
        A_GENEROUS_CONTEXT_CAP,
    );
    (repo, source)
}

const ACTIVITY_TIMEOUT: Duration = Duration::from_secs(30);
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

/// The identity a daemon serves `connection.ConnectionService` on. Fixed `daemon-` prefix, not a
/// lookup — see `docs/ft/web/daemon-selector-livekit-rpc.md`.
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

/// The `livekit:` block a daemon needs to host session rooms. `None` writes no block at all, which
/// is the unconfigured operator whose sessions must still start.
fn livekit_yaml_block(ws_url: Option<&str>) -> String {
    match ws_url {
        Some(url) => format!(
            "livekit:\n  enabled: true\n  url: {url}\n  api_key: {LK_API_KEY}\n  \
             api_secret: {LK_API_SECRET}\n  common_room: {COMMON_ROOM}\n"
        ),
        None => String::new(),
    }
}

fn write_daemon_yaml(
    ws_url: Option<&str>,
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

/// The credential a signed-in browser would present, signed with the deployment secret this
/// daemon's config carries — the same `livekit.api_secret` every daemon verifies session tokens
/// with. [`TEST_TOKEN`] is a bare literal the stubbed user resolver recognises, which is enough to
/// pass an RPC here but carries no signature: split wiring mints the agent a credential of its own
/// from the caller's claims, so the caller's has to be one that verifies.
fn a_caller_token_signed_with_the_deployment_secret() -> String {
    tddy_github::SessionTokenSigner::new(LK_API_SECRET.as_bytes()).mint_access(
        &tddy_github::GitHubUser {
            id: 4242,
            login: "testuser".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/4242?v=4".to_string(),
            name: "Test User".to_string(),
        },
    )
}

/// The daemon running the agent: it hosts the session room and answers tool calls in it.
struct FacilitatingDaemon {
    service: ConnectionServiceImpl,
    config: DaemonConfig,
    sessions_base: PathBuf,
    staging_base: PathBuf,
    ws_url: String,
    _livekit: Option<LiveKitTestkit>,
    _sessions: tempfile::TempDir,
    _staging: tempfile::TempDir,
    _config: tempfile::TempDir,
    _repo: tempfile::TempDir,
    _stubs: tempfile::TempDir,
}

impl FacilitatingDaemon {
    async fn with_livekit() -> Self {
        let livekit = LiveKitTestkit::start()
            .await
            .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
        let ws_url = livekit.get_ws_url();
        Self::build(Some(ws_url.clone()), Some(livekit)).await
    }

    async fn without_livekit() -> Self {
        Self::build(None, None).await
    }

    async fn build(ws_url: Option<String>, livekit: Option<LiveKitTestkit>) -> Self {
        let os_user = std::env::var("USER").expect("USER required");
        let repo_dir = tempfile::tempdir().unwrap();
        create_test_repo_with_origin(repo_dir.path());

        // A room belongs to a session that runs an agent, so every session here spawns one. The stub
        // reads stdin forever, which is the shape a PTY session needs; without it each start would
        // fail reaching for `claude` on PATH, for reasons that have nothing to do with rooms.
        let stub_dir = tempfile::tempdir().unwrap();
        let claude_stub = a_stub_agent_script(stub_dir.path(), "stub-claude.sh")
            .then_reading_stdin()
            .build();

        let (config_dir, config_path) =
            write_daemon_yaml(ws_url.as_deref(), &os_user, &claude_stub);
        let config = DaemonConfig::load(&config_path).expect("daemon.yaml must load");

        let sessions = tempfile::tempdir().unwrap();
        register_project(&sessions.path().join("projects"), repo_dir.path());
        let base = sessions.path().to_path_buf();
        let resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        let user_resolver: UserResolver =
            Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));

        let staging = tempfile::tempdir().unwrap();
        let service = ConnectionServiceImpl::new(
            config.clone(),
            resolver,
            sessions.path().to_path_buf(),
            user_resolver,
            None,
            None,
            None,
            Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
        )
        .with_staging_base_dir(staging.path().to_path_buf());

        Self {
            service,
            config,
            sessions_base: sessions.path().to_path_buf(),
            staging_base: staging.path().to_path_buf(),
            ws_url: ws_url.unwrap_or_default(),
            _livekit: livekit,
            _sessions: sessions,
            _staging: staging,
            _config: config_dir,
            _repo: repo_dir,
            _stubs: stub_dir,
        }
    }

    /// Create the agent session whose room is under test.
    ///
    /// `claude-cli`, not `workspace`: a session room belongs to the daemon running a session's
    /// agent, and a workspace session has none — it is a checkout with nobody to serve.
    async fn start_agent_session(&self) -> StartSessionResponse {
        self.service
            .start_session(Request::new(an_agent_session_request()))
            .await
            .expect("an agent session must start")
            .into_inner()
    }

    /// A checkout with no agent, for the case that must *not* get a room.
    async fn start_workspace_session(&self) -> StartSessionResponse {
        self.service
            .start_session(Request::new(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: TEST_PROJECT_ID.to_string(),
                session_type: "workspace".to_string(),
                ..Default::default()
            }))
            .await
            .expect("a workspace session must start")
            .into_inner()
    }

    /// The same session, started with one attachment already staged for it.
    ///
    /// Attachments arrive by reference, not by value: the bytes are uploaded to a staging area first
    /// and the request names the batch. Staging here through the production writer rather than
    /// dropping a file in place is what makes the `.staged-complete` marker real — a batch without
    /// one is refused, and a fixture that skipped it would pass for the wrong reason.
    async fn start_agent_session_with_attachment(
        &self,
        basename: &str,
        contents: &[u8],
    ) -> StartSessionResponse {
        let os_user = std::env::var("USER").expect("USER required");
        let staging_root =
            tddy_daemon::session_attachment_staging::staging_root_for(&os_user, &self.staging_base);
        tddy_daemon::session_attachment_staging::write_staged_chunk(
            &staging_root,
            STAGING_ID,
            basename,
            contents,
            true,
        )
        .expect("staging an attachment must succeed");

        self.service
            .start_session(Request::new(StartSessionRequest {
                attachments: vec![SessionAttachment {
                    basename: basename.to_string(),
                    source: Some(AttachmentSource::Staged(StagedAttachmentRef {
                        daemon_instance_id: INSTANCE_ID.to_string(),
                        staging_id: STAGING_ID.to_string(),
                        file_name: basename.to_string(),
                    })),
                }],
                ..an_agent_session_request()
            }))
            .await
            .expect("an agent session with an attachment must start")
            .into_inner()
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

// ---------------------------------------------------------------------------
// Room probes
// ---------------------------------------------------------------------------

/// A participant that joins the session room the way another agent would: no service of its own,
/// just a subscription to the activity topic and an RPC client aimed at the facilitating daemon.
struct AgentProbe {
    room: Arc<Room>,
    activity: tokio::sync::mpsc::UnboundedReceiver<ReceivedActivity>,
}

/// One broadcast as it arrived: the bytes off the data channel, alongside the decoded message.
///
/// The raw payload is kept because "an event carries no file paths" is a claim about what crosses the
/// wire — re-encoding a decoded message and inspecting that would be asserting on the test's own work
/// rather than on what the daemon actually sent.
#[derive(Debug, Clone, PartialEq)]
struct ReceivedActivity {
    raw: Vec<u8>,
    event: WorktreeActivityEvent,
}

impl AgentProbe {
    async fn join(daemon: &FacilitatingDaemon, room_name: &str, identity: &str) -> Self {
        let token = daemon
            ._livekit
            .as_ref()
            .expect("a probe needs the testkit")
            .generate_token(room_name, identity)
            .expect("LiveKit token for an agent probe");
        let (room, events) = Room::connect(&daemon.ws_url, &token, RoomOptions::default())
            .await
            .unwrap_or_else(|e| panic!("probe {identity} must join {room_name}: {e}"));
        let room = Arc::new(room);
        let activity = spawn_activity_subscription(events);
        Self { room, activity }
    }

    fn remote_identities(&self) -> Vec<String> {
        let mut identities: Vec<String> = self
            .room
            .remote_participants()
            .values()
            .map(|p| p.identity().to_string())
            .collect();
        identities.sort();
        identities
    }

    fn room_metadata(&self) -> serde_json::Value {
        let raw = self.room.metadata();
        serde_json::from_str(&raw).unwrap_or_else(|e| {
            panic!("a session room's metadata must be JSON; the room carried {raw:?}: {e}")
        })
    }

    fn rpc_to_daemon(&self) -> RpcClient {
        LiveKitRpcClientFactory::for_room(self.room.clone()).client(rpc_identity(INSTANCE_ID))
    }

    async fn next_activity(&mut self) -> ReceivedActivity {
        tokio::time::timeout(ACTIVITY_TIMEOUT, self.activity.recv())
            .await
            .expect("a worktree activity event must arrive within the timeout")
            .expect("the activity subscription must stay open")
    }

    /// The next `commit` event, discarding activity of other kinds on the way.
    ///
    /// A commit is not one observable transition. `git add` stages the file into a diff that
    /// `git diff --numstat HEAD` reports, so a poll landing in the few milliseconds before
    /// `git commit` legitimately announces a files-changed event first — and the commit that clears
    /// the working tree then announces a second one behind it. Waiting for the kind the test caused
    /// keeps the assertion about the behaviour rather than about where the tick landed, and costs no
    /// determinism: the awaited event is guaranteed to follow.
    async fn next_commit(&mut self) -> ReceivedActivity {
        self.next_activity_of(WorktreeActivityKind::Commit).await
    }

    async fn next_activity_of(&mut self, kind: WorktreeActivityKind) -> ReceivedActivity {
        let mut discarded: Vec<WorktreeActivityKind> = Vec::new();
        let awaited = tokio::time::timeout(ACTIVITY_TIMEOUT, async {
            loop {
                let received = self
                    .activity
                    .recv()
                    .await
                    .expect("the activity subscription must stay open");
                if received.event.kind() == kind {
                    return received;
                }
                discarded.push(received.event.kind());
            }
        })
        .await;
        awaited.unwrap_or_else(|_| {
            panic!("a {kind:?} event must arrive within {ACTIVITY_TIMEOUT:?}; saw {discarded:?}")
        })
    }

    /// Waits until the room's metadata reports exactly `count` untracked files.
    ///
    /// The one thing an *unannounced* change is observable through. A poll that publishes nothing
    /// still writes the room's metadata when the checkout moved, so this is positive proof that a
    /// poll ran and took the change in — without it, a stalled poll task is indistinguishable from
    /// any number of correctly silent ones.
    async fn await_untracked_count(&self, count: u64) {
        eventually_awaiting(
            &format!("the room's metadata to report {count} untracked file(s)"),
            ACTIVITY_TIMEOUT,
            || async {
                let metadata = self.room_metadata();
                (metadata["untracked_files"] == serde_json::json!(count))
                    .then_some(())
                    .ok_or_else(|| format!("the room's metadata was {metadata}"))
            },
        )
        .await;
    }
}

/// Decodes `worktree.activity` packets off a room's event stream into typed events.
///
/// Deliberately spawned at join time rather than read on demand: a data packet delivered before the
/// test starts listening is gone, and the whole point of the broadcast is that it reaches whoever is
/// already in the room.
fn spawn_activity_subscription(
    mut events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
) -> tokio::sync::mpsc::UnboundedReceiver<ReceivedActivity> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let RoomEvent::DataReceived { payload, topic, .. } = event else {
                continue;
            };
            if topic.as_deref() != Some(WORKTREE_ACTIVITY_TOPIC) {
                continue;
            }
            let raw = payload.to_vec();
            let event = WorktreeActivityEvent::decode(&raw[..])
                .expect("a worktree.activity payload must decode as a WorktreeActivityEvent");
            if tx.send(ReceivedActivity { raw, event }).is_err() {
                return;
            }
        }
    });
    rx
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
            "connection.ConnectionService",
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

async fn read_host_document_in_room(
    client: &RpcClient,
    session_id: &str,
    relative_path: &str,
) -> ReadHostDocumentResponse {
    let bytes = tokio::time::timeout(
        CALL_TIMEOUT,
        client.call_unary(
            "connection.ConnectionService",
            "ReadHostDocument",
            ReadHostDocumentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                scope: HostDocumentScope::SessionArtifact as i32,
                relative_path: relative_path.to_string(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
    )
    .await
    .expect("ReadHostDocument in the session room must return within the timeout")
    .expect("ReadHostDocument in the session room must succeed");
    ReadHostDocumentResponse::decode(&bytes[..]).expect("ReadHostDocumentResponse must decode")
}

/// Commits `contents` at `path` in the worktree and yields the resulting HEAD sha.
fn commit_a_file(worktree: &Path, path: &str, contents: &str) -> String {
    std::fs::write(worktree.join(path), contents).expect("write into the worktree");
    git(worktree, &["add", path]);
    git(worktree, &["commit", "-m", &format!("add {path}")]);
    head_commit_of(worktree)
}

/// Replaces [`SEEDED_FILE`]'s contents in one step no poll can catch half-done.
///
/// `std::fs::write` truncates before it writes, so a poll landing in between measures the file as
/// empty and announces line counts describing a state nobody asked for. Writing beside the target
/// and renaming makes the change atomic; the temporary is untracked, and an untracked file alone is
/// deliberately not activity (PRD FR5), so it passes through announcing nothing.
fn rewrite_the_seeded_file(worktree: &Path, contents: &str) {
    let staged = worktree.join(format!(".{SEEDED_FILE}.partial"));
    std::fs::write(&staged, contents).expect("write the replacement beside the tracked file");
    std::fs::rename(&staged, worktree.join(SEEDED_FILE)).expect("swap the replacement into place");
}

// ---------------------------------------------------------------------------
// AC1 — the facilitating daemon is first into the room
//
// Now proved at the connect that opens the room rather than at the start that no longer does:
// `a_session_gets_its_room_when_something_first_connects_to_it`, below. The property is unchanged —
// a participant learns where the room is from the same call that put the daemon in it — but the
// moment it becomes observable moved with the open.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// AC12 — a session with no agent has no facilitating daemon, and no room
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_workspace_session_runs_no_agent_and_so_gets_no_room() {
    // Given a daemon that does host session rooms
    let daemon = FacilitatingDaemon::with_livekit().await;

    // When it creates a checkout with no agent
    let started = daemon.start_workspace_session().await;

    // Then nobody is hosting a room for it. A workspace session is the codebase half of a split
    // session or a standalone checkout; either way its agent — if it has one at all — runs on
    // another daemon, and a room here would be one this daemon could serve to nobody.
    let probe = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-workspace",
    )
    .await;
    assert_eq!(
        probe.remote_identities(),
        Vec::<String>::new(),
        "a workspace session must have no facilitating daemon in its room"
    );
}

// ---------------------------------------------------------------------------
// AC2 — the split agent is wired to the session room, not the lobby
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_split_agent_is_wired_to_the_session_room_rather_than_the_lobby() {
    // Given a worktree-backed session on the facilitating daemon
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon.start_agent_session().await;
    let session_dir = unified_session_dir_path(&daemon.sessions_base, &started.session_id);
    let a_project_with_no_guidance = a_codebase_holding_no_agent_guidance();

    // When its agent's remote-tool wiring is prepared, with the checkout placed on another daemon
    let wiring = tddy_daemon::split_session::prepare_split_agent_wiring(
        &daemon.config,
        &session_dir,
        &true_bin(),
        &tddy_daemon::split_session::SplitSpawnTarget {
            session_id: &started.session_id,
            codebase_instance_id: "some-other-codebase-host",
            codebase_session_id: "0199bbbb-0000-7000-8000-00000000000b",
            session_token: &a_caller_token_signed_with_the_deployment_secret(),
        },
        &[],
        CLAUDE_CONTEXT_GLOBS,
        &a_project_with_no_guidance.1,
    )
    .expect("split agent wiring must be preparable for an agent session");
    let env: std::collections::HashMap<String, String> = wiring.env.into_iter().collect();

    // Then the agent is pointed at *this* session's room — the one this daemon hosts because it
    // runs the agent — and not at the lobby every daemon shares, nor at a room named after the
    // codebase session on a host that hosts no room at all
    assert_eq!(
        env.get("TDDY_REMOTE_LIVEKIT_ROOM").map(String::as_str),
        Some(session_room_name(&started.session_id).as_str()),
        "a split agent must join the room its own facilitating daemon hosts"
    );

    // ...and the token it was given lands there too. A LiveKit join token names its room in the
    // grant and `Room::connect` takes no room argument, so where this connection ends up *is* the
    // only room the token can reach — an agent running model-authored code has no second room to ask
    // for, least of all the lobby every other daemon in the fleet is addressable in.
    let token = env
        .get("TDDY_REMOTE_LIVEKIT_TOKEN")
        .expect("the wiring must mint a join token");
    let (joined, _events) = Room::connect(&daemon.ws_url, token, RoomOptions::default())
        .await
        .expect("the agent's token must admit the room it was minted for");
    assert_eq!(
        joined.name(),
        session_room_name(&started.session_id),
        "the agent's token must place it in the session room, not the lobby"
    );
}

#[tokio::test]
#[serial]
async fn a_split_agent_addresses_the_daemon_that_hosts_its_room() {
    // Given a worktree-backed session on the facilitating daemon
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon.start_agent_session().await;
    let session_dir = unified_session_dir_path(&daemon.sessions_base, &started.session_id);
    let a_project_with_no_guidance = a_codebase_holding_no_agent_guidance();

    // When its agent's remote-tool wiring is prepared, with the checkout placed on another daemon
    let wiring = tddy_daemon::split_session::prepare_split_agent_wiring(
        &daemon.config,
        &session_dir,
        &true_bin(),
        &tddy_daemon::split_session::SplitSpawnTarget {
            session_id: &started.session_id,
            codebase_instance_id: "some-other-codebase-host",
            codebase_session_id: "0199bbbb-0000-7000-8000-00000000000b",
            session_token: &a_caller_token_signed_with_the_deployment_secret(),
        },
        &[],
        CLAUDE_CONTEXT_GLOBS,
        &a_project_with_no_guidance.1,
    )
    .expect("split agent wiring must be preparable for an agent session");
    let env: std::collections::HashMap<String, String> = wiring.env.into_iter().collect();

    // Then the participant it is told to call is this daemon's — the one that opened the room and
    // serves its RPC surface. The codebase host holds the files, but it hosts no room and joins
    // none, so an agent addressed at it waits in a room that identity will never enter.
    assert_eq!(
        env.get("TDDY_REMOTE_SERVER_IDENTITY").map(String::as_str),
        Some(rpc_identity(INSTANCE_ID).as_str()),
        "a split agent must address the identity hosting the room it was given"
    );
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

// ---------------------------------------------------------------------------
// AC4 — one publish reaches every participant
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_commit_reaches_both_agent_participants_from_a_single_publish() {
    // Given two agents in the session room — the coding agent, and the second agent this room exists
    // to make possible
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon.a_session_being_connected_to().await;
    let worktree = daemon.worktree_of(&started.session_id);
    let mut coder = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-coder",
    )
    .await;
    let mut explorer = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-explorer",
    )
    .await;

    // When a commit lands in the worktree
    let head = commit_a_file(&worktree, "committed.txt", "work in progress");

    // Then both are told, from one broadcast — the daemon never learned either identity
    let to_coder = coder.next_commit().await;
    let to_explorer = explorer.next_commit().await;
    assert_eq!(to_coder.event.head_commit, head);
    assert_eq!(
        to_explorer, to_coder,
        "a broadcast must deliver the identical bytes to every participant"
    );
}

// ---------------------------------------------------------------------------
// AC5 — events carry counts, never content
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn writing_a_tracked_file_broadcasts_counts_without_paths_or_contents() {
    // Given an agent watching a worktree whose tracked file was committed before the room opened, so
    // the daemon's baseline already accounts for it and nothing is waiting to be announced
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon.a_session_being_connected_to().await;
    let worktree = daemon.worktree_of(&started.session_id);
    let mut agent = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-writer",
    )
    .await;

    // When two lines are added to that tracked file
    rewrite_the_seeded_file(&worktree, "one\ntwo\nthree\nfour\n");

    // Then the event reports what `git diff --numstat HEAD` reports, and nothing more
    let received = agent.next_activity().await;
    assert_eq!(received.event.kind(), WorktreeActivityKind::FilesChanged);
    assert_eq!(received.event.changed_files, 1);
    assert_eq!(received.event.lines_added, 2);
    assert_eq!(received.event.lines_removed, 0);

    // ...and the bytes that crossed the wire carry no path and no content. The room exists to say
    // *that* the checkout moved; reading it is what the file-access RPCs in this same room are for.
    // Protobuf keeps strings unescaped on the wire, so a leak would appear verbatim here.
    let on_the_wire = String::from_utf8_lossy(&received.raw).into_owned();
    assert!(
        !on_the_wire.contains(SEEDED_FILE),
        "an activity event must carry no file path; wire bytes held {on_the_wire:?}"
    );
    assert!(
        !on_the_wire.contains("three"),
        "an activity event must carry no file contents; wire bytes held {on_the_wire:?}"
    );
}

// ---------------------------------------------------------------------------
// AC6 — silence when nothing changed
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn polls_that_find_no_tracked_change_leave_the_event_sequence_unbroken() {
    // Given an agent watching a worktree, with one change already announced
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon.a_session_being_connected_to().await;
    let worktree = daemon.worktree_of(&started.session_id);
    let mut agent = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-quiet",
    )
    .await;
    rewrite_the_seeded_file(&worktree, "one\ntwo\nthree\n");
    let first = agent.next_activity().await;

    // When a file git has never been told about appears — provably taken in by a poll, since it
    // reaches the room's metadata — and only then is the tracked file changed again
    std::fs::write(worktree.join("scratch.txt"), "notes\n").expect("add an untracked file");
    agent.await_untracked_count(1).await;
    rewrite_the_seeded_file(&worktree, "one\ntwo\nthree\nfour\n");

    // Then the next event is the very next sequence number: the polls that measured the untracked
    // file published nothing at all. A gap here would mean the daemon announced a `files=0 +0 -0`
    // event no receiver could act on — and burned a `seq`, which the wire contract documents as
    // meaning an event was lost.
    let second = agent.next_activity().await;
    assert_eq!(
        second.event.seq,
        first.event.seq + 1,
        "a poll that found no tracked change must publish nothing; sequence jumped from {} to {}",
        first.event.seq,
        second.event.seq
    );
}

// ---------------------------------------------------------------------------
// AC7 — room metadata is the snapshot a late joiner needs
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_late_joiner_reads_the_current_worktree_summary_from_room_metadata() {
    // Given a worktree carrying an uncommitted edit with a commit landed on top of it, and an agent
    // already in the room to say when the daemon has taken both in
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon.a_session_being_connected_to().await;
    let worktree = daemon.worktree_of(&started.session_id);
    let mut early = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-early",
    )
    .await;

    rewrite_the_seeded_file(&worktree, "one\ntwo\nthree\n");
    let head = commit_a_file(&worktree, "shipped.txt", "shipped\n");
    early.next_commit().await;

    // When a second agent joins now, having observed none of it
    let late = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-late",
    )
    .await;

    // Then the room itself tells it where the checkout stands. Metadata is written before the event
    // that announces it, so the commit `early` has already seen means the snapshot is at least that
    // new — and the commit was the last thing to happen to this checkout, so it is exactly that new.
    let metadata = late.room_metadata();
    assert_eq!(metadata["head_commit"], head);
    assert_eq!(metadata["changed_files"], 1);
    assert_eq!(metadata["lines_added"], 1);
    assert_eq!(metadata["lines_removed"], 0);
    assert_eq!(
        metadata["changed_paths"],
        serde_json::json!([SEEDED_FILE]),
        "the room metadata is where the changed file list lives, since events carry no paths"
    );
}

// ---------------------------------------------------------------------------
// AC10 / AC11 — the facilitating daemon serves the session's attachments
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_participant_reads_an_attachment_through_read_host_document_in_the_session_room() {
    // Given a worktree-backed session started with an attachment
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon
        .a_session_being_connected_to_with_attachment("spec.md", b"# the shared spec\n")
        .await;
    let probe = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-attachment",
    )
    .await;

    // When a participant of the session room asks the facilitating daemon for it
    let response = read_host_document_in_room(
        &probe.rpc_to_daemon(),
        &started.session_id,
        "attachments/spec.md",
    )
    .await;

    // Then it gets the exact bytes. The daemon that holds the checkout is the one that holds what was
    // attached to it, and it hands both to the same room — an agent restricted to this room needs no
    // second host to be complete.
    assert_eq!(response.data, b"# the shared spec\n");
}

#[tokio::test]
#[serial]
async fn room_metadata_lists_the_sessions_attachment_basenames() {
    // Given a worktree-backed session started with an attachment
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon
        .a_session_being_connected_to_with_attachment("design.txt", b"shapes and colours")
        .await;

    // When an agent joins the room
    let probe = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-metadata",
    )
    .await;

    // Then the room itself tells it what is shared, so discovering an attachment costs no round trip
    assert_eq!(
        probe.room_metadata()["attachments"],
        serde_json::json!(["design.txt"]),
        "room metadata must list the attachments the facilitating daemon serves"
    );
}

// ---------------------------------------------------------------------------
// AC9 — no LiveKit, no room, still a working session
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_daemon_without_livekit_credentials_starts_a_session_and_creates_no_room() {
    // Given a daemon configured with no LiveKit at all
    let daemon = FacilitatingDaemon::without_livekit().await;

    // When it starts an agent session
    let started = daemon.start_agent_session().await;

    // Then the worktree is real and the session is usable — the room is an addition, not a
    // prerequisite, and an operator who never configured LiveKit keeps the daemon they had
    assert!(
        daemon.worktree_of(&started.session_id).exists(),
        "the worktree must be created whether or not a room could be hosted"
    );
    assert_eq!(
        started.livekit_room, "",
        "a daemon with no LiveKit credentials must not claim to have hosted a room"
    );
}

// ---------------------------------------------------------------------------
// Session creation is a local operation
//
// A session is created out of a git checkout and a session directory, both on this host. LiveKit
// carries what the session says about itself afterwards, so *creating* one must not depend on
// reaching a LiveKit server — configured or not, up or not. The room a session is facilitated in is
// opened the first time something actually connects to the session over LiveKit, not before.
//
// These need no testkit container: their whole subject is that nothing is dialled.
// ---------------------------------------------------------------------------

/// A LiveKit address that accepts connections and answers nothing.
///
/// The operator's failure exactly: `livekit:` configured, the server unreachable, every request
/// hanging until something upstream gives up. An address nothing listens on would be refused
/// instantly and would prove far less — a start that survived a fast `ECONNREFUSED` can still block
/// for minutes against a server that merely never replies.
///
/// The count of accepted connections *is* the assertion. Any contact at all shows up here, whether
/// the daemon would have created a room, joined one, or bridged a PTY into the lobby.
struct ALiveKitNobodyAnswers {
    addr: std::net::SocketAddr,
    accepted: Arc<std::sync::atomic::AtomicUsize>,
    _accepting: tokio::task::JoinHandle<()>,
}

async fn a_livekit_nobody_answers() -> ALiveKitNobodyAnswers {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a stand-in LiveKit must be able to bind a loopback port");
    let addr = listener
        .local_addr()
        .expect("a bound listener must know its address");
    let accepted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = Arc::clone(&accepted);
    let accepting = tokio::spawn(async move {
        // Every accepted stream is kept, never read from and never written to: dropping it would
        // close the connection and let the caller fail fast, which is the one thing this server
        // must not do.
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            held.push(stream);
        }
    });
    ALiveKitNobodyAnswers {
        addr,
        accepted,
        _accepting: accepting,
    }
}

impl ALiveKitNobodyAnswers {
    fn ws_url(&self) -> String {
        format!("ws://{}", self.addr)
    }

    fn connections_accepted(&self) -> usize {
        self.accepted.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl FacilitatingDaemon {
    /// A daemon configured with LiveKit at [`ALiveKitNobodyAnswers`].
    async fn with_livekit_that_never_answers(livekit: &ALiveKitNobodyAnswers) -> Self {
        Self::build(Some(livekit.ws_url()), None).await
    }

    /// Start an agent session, failing rather than hanging if the start blocks on the network.
    ///
    /// 10s: a start cuts a git worktree and spawns a stub agent, so it is not instant — but it is
    /// bounded by local work alone, and a start that waits on an unanswering socket exceeds this by
    /// whatever that layer's own timeout is. The timeout is the assertion's teeth: without it a
    /// regression here is a suite that hangs rather than a test that fails.
    async fn starts_an_agent_session_within(&self, patience: Duration) -> StartSessionResponse {
        tokio::time::timeout(patience, self.start_agent_session())
            .await
            .expect("StartSession must not block waiting on LiveKit")
    }
}

#[tokio::test]
async fn starting_a_session_dials_livekit_not_at_all() {
    // Given a daemon whose LiveKit is configured, reachable on the wire, and answers nothing
    let livekit = a_livekit_nobody_answers().await;
    let daemon = FacilitatingDaemon::with_livekit_that_never_answers(&livekit).await;

    // When it starts a session whose agent it runs
    let started = daemon
        .starts_an_agent_session_within(Duration::from_secs(10))
        .await;

    // Then the checkout the session is made of is real...
    assert!(
        daemon.worktree_of(&started.session_id).exists(),
        "a session's worktree must be cut whether or not LiveKit can be reached"
    );

    // ...and nothing was ever dialled. Asserted as the absence of a connection rather than as the
    // presence of a session: a start that contacted an unreachable server and recovered from it
    // would still have blocked for as long as that took, which is the defect itself.
    assert_eq!(
        livekit.connections_accepted(),
        0,
        "creating a session must not contact LiveKit"
    );
}

/// A LiveKit address nothing is listening on.
///
/// A bound port, released before it is ever used: the operating system will not hand the same one
/// out again while this test runs, so a dial to it is refused rather than answered by whatever
/// happens to be there. `ECONNREFUSED` arrives immediately, which is what makes this the
/// *unreachable-and-says-so* case rather than the hanging one [`ALiveKitNobodyAnswers`] covers.
async fn a_livekit_address_nothing_listens_on() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binding a loopback port must succeed");
    let addr = listener
        .local_addr()
        .expect("a bound listener must know its address");
    drop(listener);
    format!("ws://{addr}")
}

impl FacilitatingDaemon {
    async fn with_livekit_nothing_listens_on() -> Self {
        Self::build(Some(a_livekit_address_nothing_listens_on().await), None).await
    }

    /// A started session whose room is open — which is what having connected to it means.
    ///
    /// Every test about what happens *inside* a session's room needs one open, and starting a
    /// session no longer opens it: connecting to the session does. Said once here rather than
    /// repeated as a second setup line in each of them.
    async fn a_session_being_connected_to(&self) -> StartSessionResponse {
        let started = self.start_agent_session().await;
        self.connect_to(&started.session_id)
            .await
            .expect("connecting to a started session must open its room");
        started
    }

    /// [`Self::a_session_being_connected_to`] for the tests whose subject is what the room says
    /// about a session's attachments.
    async fn a_session_being_connected_to_with_attachment(
        &self,
        basename: &str,
        contents: &[u8],
    ) -> StartSessionResponse {
        let started = self
            .start_agent_session_with_attachment(basename, contents)
            .await;
        self.connect_to(&started.session_id)
            .await
            .expect("connecting to a started session must open its room");
        started
    }

    /// Connect to a session, as a client reaching it over LiveKit does.
    async fn connect_to(
        &self,
        session_id: &str,
    ) -> Result<tddy_service::proto::connection::ConnectSessionResponse, tddy_rpc::Status> {
        self.service
            .connect_session(Request::new(ConnectSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|response| response.into_inner())
    }

    /// What the LiveKit **server** says about a room, or `None` when it has never heard of it.
    ///
    /// Read from the server's own room list rather than by joining: joining a room that does not
    /// exist creates it, which would make the question change its own answer.
    async fn room_on_the_server(&self, room: &str) -> Option<LiveKitRoomInfo> {
        tddy_daemon::livekit_rooms_stream::room_roster_from_config(self.config.livekit.as_ref())
            .list_rooms()
            .await
            .expect("the LiveKit server must answer its room list")
            .into_iter()
            .find(|listed| listed.name == room)
    }
}

#[tokio::test]
#[serial]
async fn a_session_gets_its_room_when_something_first_connects_to_it() {
    // Given a daemon that hosts session rooms, and a session it has started
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon.start_agent_session().await;
    let room = session_room_name(&started.session_id);

    // ...which the LiveKit server has never been told about, because creating a session is local
    // work and touches LiveKit nowhere
    assert_eq!(
        daemon.room_on_the_server(&room).await,
        None,
        "starting a session must not create its room"
    );

    // When a client connects to that session
    daemon
        .connect_to(&started.session_id)
        .await
        .expect("connecting to a started session must succeed");

    // Then the room exists and its facilitating daemon is in it, alone. Deferring the open moved
    // *when* that happens without weakening it: a participant learns where the room is from the
    // same call that put the daemon in it, so it still cannot arrive first (PRD FR2).
    let probe = AgentProbe::join(&daemon, &room, "probe-first-joiner").await;
    assert_eq!(
        probe.remote_identities(),
        vec![rpc_identity(INSTANCE_ID)],
        "the facilitating daemon must be the sole participant of the room its connect opened"
    );
}

#[tokio::test]
#[serial]
async fn connecting_again_goes_on_using_the_room_the_first_connect_opened() {
    // Given a session whose room a first connect opened, with a participant already in it
    let daemon = FacilitatingDaemon::with_livekit().await;
    let started = daemon.start_agent_session().await;
    daemon
        .connect_to(&started.session_id)
        .await
        .expect("the first connect must succeed");
    let mut probe = AgentProbe::join(
        &daemon,
        &session_room_name(&started.session_id),
        "probe-second-connect",
    )
    .await;
    let worktree = daemon.worktree_of(&started.session_id);
    commit_a_file(&worktree, "first.txt", "one\n");
    let before = probe.next_commit().await;

    // When a second client connects to the same session
    daemon
        .connect_to(&started.session_id)
        .await
        .expect("the second connect must succeed");

    // Then it is the same room, still measuring the same checkout: the event sequence carries on
    // from where it was rather than starting over, which a room replaced under this session's id
    // could not do — a replacement aborts the poll loop that owns the counter and begins at zero.
    //
    // Greater-than rather than a literal: how many events land between two commits depends on where
    // the poll interval falls, and a staged-but-not-yet-committed file is legitimately an event of
    // its own. What is being asserted is that the counter continued at all.
    commit_a_file(&worktree, "second.txt", "two\n");
    let after = probe.next_commit().await;
    assert!(
        after.event.seq > before.event.seq,
        "the room's event sequence must continue across a second connect; it went from {} to {}",
        before.event.seq,
        after.event.seq
    );
}

#[tokio::test]
async fn connecting_to_a_session_fails_when_livekit_cannot_be_reached() {
    // Given a daemon whose LiveKit is configured at an address nothing answers on, and a session it
    // started there regardless
    let daemon = FacilitatingDaemon::with_livekit_nothing_listens_on().await;
    let started = daemon
        .starts_an_agent_session_within(Duration::from_secs(10))
        .await;

    // When a client connects to it
    let refused = daemon
        .connect_to(&started.session_id)
        .await
        .expect_err("connecting must fail when the room it needs cannot be created");

    // Then it is told so, naming the room it could not get. Creating a session stopped depending on
    // LiveKit; *reaching* one over LiveKit did not, and a client that asked for a room and has none
    // must hear about it rather than be handed coordinates nothing is serving.
    assert!(
        refused
            .message
            .contains(&session_room_name(&started.session_id)),
        "the failure must name the room that could not be created, got: {}",
        refused.message
    );
}
