//! Acceptance: cross-host attachments — forwarding to a peer that serves under its **production**
//! LiveKit identity.
//!
//! PRD: `docs/ft/web/1-WIP/PRD-2026-08-01-session-attach-ui.md`
//! Changeset: `docs/dev/1-WIP/2026-08-01-session-attach-ui.md`
//!
//! A daemon joins the common room **twice**: a discovery participant under the bare instance id
//! (`livekit_peer_discovery::spawn_common_room_discovery_task`) which publishes the advertisement
//! and serves no RPC, and an RPC participant under `daemon-{instance_id}` (`main.rs`) which serves
//! `connection.ConnectionService`. Every other forwarding test in this repo stands up a peer whose
//! **serving** identity is the bare id, so none of them exercises the identity a real peer answers
//! on. This suite does, which is why it is separate from
//! `tests/staging_forwarding_acceptance.rs`.
//!
//! These need the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`); they are
//! `#[serial]` so they own the container alone.
//!
//! Single-host behavior lives in `tests/session_attach_staging_scope_acceptance.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use livekit::prelude::RoomOptions;
use serial_test::serial;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::{
    spawn_common_room_discovery_task, CommonRoomPeerRegistry, LiveKitDiscoveryHandles,
    LiveKitEligibleDaemonSource,
};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_livekit::LiveKitParticipant;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::connection::{
    session_attachment::Source as AttachmentSource, start_session_event::Event as StartEvent,
    AttachmentMaterializationProgress, ConnectionService as ConnectionServiceTrait,
    HostDocumentChunk, HostDocumentScope, ListEligibleDaemonsRequest, ReadHostDocumentRequest,
    SessionAttachment, StagedAttachmentRef, StartSessionRequest,
    UploadStagedAttachmentChunkRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const ROOM: &str = "attach-cross-host-room";
/// Local daemon A — the host the browser is connected to and stages its bytes on.
const LOCAL_INSTANCE_ID: &str = "attach-cross-host-local";
/// Peer daemon B — the host a session may be started on instead.
const PEER_INSTANCE_ID: &str = "attach-cross-host-peer";
const LK_API_KEY: &str = "devkey";
const LK_API_SECRET: &str = "secret";
const TEST_PROJECT_ID: &str = "attach-cross-host-proj";
const STAGING_ID: &str = "cccccccc-cccc-7ccc-8ccc-cccccccccccc";

/// The identity a daemon actually serves `connection.ConnectionService` on. Fixed `daemon-` prefix,
/// not a lookup — see `docs/ft/web/daemon-selector-livekit-rpc.md`.
fn rpc_identity(instance_id: &str) -> String {
    format!("daemon-{instance_id}")
}

fn true_bin() -> &'static str {
    if cfg!(target_os = "macos") {
        "/usr/bin/true"
    } else {
        "/bin/true"
    }
}

fn write_livekit_daemon_yaml(
    ws_url: &str,
    daemon_instance_id: &str,
    os_user: &str,
) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    let true_path = true_bin();
    let yaml = format!(
        r#"
daemon_instance_id: {daemon_instance_id}
users:
  - github_user: "testuser"
    os_user: "{os_user}"
allowed_tools:
  - path: {true_path}
    label: t
livekit:
  url: {ws_url}
  api_key: {LK_API_KEY}
  api_secret: {LK_API_SECRET}
  common_room: {ROOM}
"#
    );
    std::fs::write(&path, yaml).unwrap();
    (dir, path)
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: cross-host-proj\n    git_url: \"\"\n    main_repo_path: {}\n",
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn create_test_repo_with_origin(dir: &Path) {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git");
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);
}

/// One daemon wired exactly the way `main.rs` wires a real one: a discovery participant on the bare
/// instance id publishing the advertisement and serving no RPC, plus a peer-routing source over the
/// same common room. The **RPC** participant on `daemon-{instance_id}` is joined separately by
/// [`serve_rpc_participant`], because that needs the finished service.
struct Daemon {
    service: ConnectionServiceImpl,
    /// Sessions/data root — where a session started on this host puts its attachments.
    sessions_base: PathBuf,
    /// Staging base — where bytes staged *to this host* land.
    staging_base: PathBuf,
    _sessions: tempfile::TempDir,
    _staging: tempfile::TempDir,
    _config: tempfile::TempDir,
}

async fn a_daemon(
    ws_url: &str,
    instance_id: &str,
    os_user: &str,
    repo: &Path,
    user_resolver: UserResolver,
) -> Daemon {
    let (config_dir, config_path) = write_livekit_daemon_yaml(ws_url, instance_id, os_user);
    let config = DaemonConfig::load(&config_path).unwrap();

    let sessions = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    register_project(&sessions.path().join("projects"), repo);
    let base = sessions.path().to_path_buf();
    let resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));

    let config_arc = Arc::new(config.clone());
    let registry = Arc::new(CommonRoomPeerRegistry::new());
    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
    spawn_common_room_discovery_task(config_arc.clone(), registry.clone(), room_slot.clone());
    let eligible: Arc<dyn tddy_daemon::multi_host::EligibleDaemonSource> = Arc::new(
        LiveKitEligibleDaemonSource::new(config_arc, registry, room_slot.clone()),
    );

    let service = ConnectionServiceImpl::new(
        config,
        resolver,
        sessions.path().to_path_buf(),
        user_resolver,
        None,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: eligible,
            common_room_livekit_room: room_slot,
        }),
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
    .with_staging_base_dir(staging.path().to_path_buf());

    Daemon {
        service,
        sessions_base: sessions.path().to_path_buf(),
        staging_base: staging.path().to_path_buf(),
        _sessions: sessions,
        _staging: staging,
        _config: config_dir,
    }
}

/// Joins the common room as `daemon-{instance_id}` serving `ConnectionService` — the identity a
/// caller must address, and the only one that answers.
async fn serve_rpc_participant(
    livekit: &LiveKitTestkit,
    ws_url: &str,
    instance_id: &str,
    service: ConnectionServiceImpl,
) -> tokio::task::JoinHandle<()> {
    let token = livekit
        .generate_token(ROOM, &rpc_identity(instance_id))
        .expect("LiveKit token for a daemon's RPC participant");
    let server = tddy_service::ConnectionServiceServer::new(service);
    let participant =
        LiveKitParticipant::connect(ws_url, &token, server, RoomOptions::default(), None, None)
            .await
            .expect("daemon joins the common room as its RPC participant");
    tokio::spawn(async move { participant.run().await })
}

/// Blocks until `service` lists `peer_instance_id` as eligible, so a route to it classifies as a
/// forward rather than an unknown host.
///
/// 45s: two daemons publish their advertisement on the common room's own metadata cadence, and a
/// cold LiveKit container has to accept both participants first.
async fn wait_until_discovered(service: &ConnectionServiceImpl, peer_instance_id: &str) {
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            let daemons = service
                .list_eligible_daemons(Request::new(ListEligibleDaemonsRequest {
                    session_token: TEST_TOKEN.to_string(),
                }))
                .await
                .expect("ListEligibleDaemons")
                .into_inner()
                .daemons;
            if daemons.iter().any(|d| d.instance_id == peer_instance_id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timeout waiting for daemon {peer_instance_id} to be discovered"));
}

/// Local daemon A plus peer daemon B, each serving RPC on its own `daemon-{id}` identity and each
/// able to route to the other: A forwards a session start to B, and B fetches staged bytes back
/// from A.
struct TwoDaemons {
    service_a: ConnectionServiceImpl,
    /// A's sessions/data root — where a session started on A puts its attachments.
    local_base: PathBuf,
    /// A's staging base — where bytes staged on the host the browser is connected to land.
    local_staging_base: PathBuf,
    /// B's staging base — where bytes staged *to the peer* land.
    peer_staging_base: PathBuf,
    /// B's sessions/data root — where a session started *on the peer* puts its attachments.
    peer_sessions_base: PathBuf,
    peer_rpc_run: tokio::task::JoinHandle<()>,
    _local_rpc_run: tokio::task::JoinHandle<()>,
    _livekit: LiveKitTestkit,
    _repo: tempfile::TempDir,
    _local: Daemon,
    _peer: Daemon,
}

async fn two_daemons() -> TwoDaemons {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let ws_url = livekit.get_ws_url();
    let os_user = std::env::var("USER").expect("USER required");

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));

    let peer = a_daemon(
        &ws_url,
        PEER_INSTANCE_ID,
        &os_user,
        repo_dir.path(),
        user_resolver.clone(),
    )
    .await;
    let local = a_daemon(
        &ws_url,
        LOCAL_INSTANCE_ID,
        &os_user,
        repo_dir.path(),
        user_resolver,
    )
    .await;

    let peer_rpc_run =
        serve_rpc_participant(&livekit, &ws_url, PEER_INSTANCE_ID, peer.service.clone()).await;
    let local_rpc_run =
        serve_rpc_participant(&livekit, &ws_url, LOCAL_INSTANCE_ID, local.service.clone()).await;

    wait_until_discovered(&local.service, PEER_INSTANCE_ID).await;
    wait_until_discovered(&peer.service, LOCAL_INSTANCE_ID).await;

    TwoDaemons {
        service_a: local.service.clone(),
        local_base: local.sessions_base.clone(),
        local_staging_base: local.staging_base.clone(),
        peer_staging_base: peer.staging_base.clone(),
        peer_sessions_base: peer.sessions_base.clone(),
        peer_rpc_run,
        _local_rpc_run: local_rpc_run,
        _livekit: livekit,
        _repo: repo_dir,
        _local: local,
        _peer: peer,
    }
}

/// Stages one complete file on the peer, by addressing the staging RPC at the peer's instance id.
async fn stage_on_peer(service_a: &ConnectionServiceImpl, file_name: &str, data: &[u8]) {
    service_a
        .upload_staged_attachment_chunk(Request::new(UploadStagedAttachmentChunkRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: PEER_INSTANCE_ID.to_string(),
            staging_id: STAGING_ID.to_string(),
            file_name: file_name.to_string(),
            data: data.to_vec(),
            last: true,
        }))
        .await
        .expect("forwarded staging upload must reach the peer");
}

/// Stages one complete file on **this** host — the browser's natural flow: bytes go to whichever
/// daemon the UI is connected to, which need not be the host that runs the session. Returns where
/// they landed, so a test can pin that a session on the peer really had to cross a host boundary.
async fn stage_on_local(env: &TwoDaemons, file_name: &str, data: &[u8]) -> PathBuf {
    env.service_a
        .upload_staged_attachment_chunk(Request::new(UploadStagedAttachmentChunkRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: String::new(),
            staging_id: STAGING_ID.to_string(),
            file_name: file_name.to_string(),
            data: data.to_vec(),
            last: true,
        }))
        .await
        .expect("staging on the local daemon must succeed");
    let os_user = std::env::var("USER").expect("USER required");
    tddy_daemon::session_attachment_staging::staging_root_for(&os_user, &env.local_staging_base)
        .join(STAGING_ID)
        .join(file_name)
}

fn peer_staged_document_request(relative_path: &str) -> ReadHostDocumentRequest {
    ReadHostDocumentRequest {
        session_token: TEST_TOKEN.to_string(),
        daemon_instance_id: PEER_INSTANCE_ID.to_string(),
        scope: HostDocumentScope::StagedAttachment.into(),
        session_id: String::new(),
        project_id: String::new(),
        relative_path: relative_path.to_string(),
    }
}

async fn drain_document(
    stream: &mut (impl Stream<Item = Result<HostDocumentChunk, Status>> + Unpin),
) -> Vec<u8> {
    let mut bytes = Vec::new();
    // 20s per frame, not per document: every frame crosses two hosts over the LiveKit data channel,
    // and a document past the unary cap is a hundred of them.
    while let Some(item) = tokio::time::timeout(Duration::from_secs(20), stream.next())
        .await
        .expect("no host-document frame arrived within the timeout")
    {
        let chunk = item.expect("forwarded host-document stream yielded an error");
        bytes.extend_from_slice(&chunk.data);
    }
    bytes
}

/// One progress event as `(basename, bytes_done, bytes_total)` — the fields a per-row progress bar
/// is drawn from, in a shape whose `assert_eq!` diff stays readable across a whole transfer.
fn progress_row(reported: AttachmentMaterializationProgress) -> (String, u64, u64) {
    (reported.basename, reported.bytes_done, reported.bytes_total)
}

fn any_attachment_written(sessions_base: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(sessions_base.join("sessions")) else {
        return false;
    };
    entries.flatten().any(|session| {
        std::fs::read_dir(session.path().join("artifacts").join("attachments"))
            .map(|mut files| files.any(|f| f.is_ok()))
            .unwrap_or(false)
    })
}

// ---------------------------------------------------------------------------
// Forwarding reaches the identity a real peer serves on
// ---------------------------------------------------------------------------

/// A daemon serves RPC on `daemon-{instance_id}`; its bare instance id belongs to the discovery
/// participant, which runs no RPC server. A forward addressed at the bare id therefore reaches
/// nothing and — with no deadline on the forward — never returns at all. This pins the forward
/// against a peer wired the way production wires one.
#[tokio::test]
#[serial]
async fn a_forwarded_rpc_reaches_a_peer_serving_under_its_daemon_prefixed_identity() {
    // Given — two daemons, the peer serving on `daemon-{id}` like a real one
    let env = two_daemons().await;

    // When — A forwards a staging upload to the peer
    // 30s: matches the forward's own deadline (`PEER_FORWARD_TIMEOUT`), so a hang shows up as this
    // test failing rather than as a suite that never finishes.
    tokio::time::timeout(
        Duration::from_secs(30),
        stage_on_peer(&env.service_a, "reached.md", b"the peer answered"),
    )
    .await
    .expect("a forwarded RPC must return, not hang, when the peer serves on daemon-{id}");

    // Then — the bytes are on the peer's staging root
    let os_user = std::env::var("USER").unwrap();
    let peer_staged =
        tddy_daemon::session_attachment_staging::staging_root_for(&os_user, &env.peer_staging_base)
            .join(STAGING_ID)
            .join("reached.md");
    assert!(
        peer_staged.exists(),
        "the forwarded upload must land on the peer at {peer_staged:?}"
    );
    assert_eq!(std::fs::read(&peer_staged).unwrap(), b"the peer answered");
}

/// A forward to a peer that has stopped answering must fail within its deadline. Without one the
/// call waits on a response that never comes, and the operator sees a Create button that never
/// finishes and never errors.
#[tokio::test]
#[serial]
async fn a_forwarded_rpc_to_a_peer_that_stopped_answering_fails_within_its_deadline() {
    // Given — two daemons, then the peer's RPC participant is taken down
    let env = two_daemons().await;
    env.peer_rpc_run.abort();

    // When — A forwards a staging upload to the now-silent peer
    // 60s: the forward's own deadline is 30s (`PEER_FORWARD_TIMEOUT`), and this outer wait exists
    // only to fail the test instead of hanging it if that deadline is missing — so it has to be
    // comfortably longer than the deadline it is checking.
    let outcome = tokio::time::timeout(
        Duration::from_secs(60),
        env.service_a.upload_staged_attachment_chunk(Request::new(
            UploadStagedAttachmentChunkRequest {
                session_token: TEST_TOKEN.to_string(),
                daemon_instance_id: PEER_INSTANCE_ID.to_string(),
                staging_id: STAGING_ID.to_string(),
                file_name: "unreachable.md".to_string(),
                data: b"never arrives".to_vec(),
                last: true,
            },
        )),
    )
    .await
    .expect("the forward must return within its own deadline rather than hang");

    // Then — it failed *on its deadline*: a bare `is_err()` would also pass if the forward were
    // refused up front (say, because the peer dropped out of the eligible list), which would leave
    // the missing deadline undetected.
    let status = outcome.expect_err("a forward to a silent peer must fail");
    assert_eq!(status.code, Code::DeadlineExceeded, "got {status:?}");
}

// ---------------------------------------------------------------------------
// Cross-host staged attachments
// ---------------------------------------------------------------------------

/// The browser stages to whichever daemon it is connected to, then may start the session on another
/// host. The session host fetches the staged bytes from the staging host, so a staged ref naming a
/// foreign daemon is now materialized rather than refused.
#[tokio::test]
#[serial]
async fn a_staged_ref_naming_another_host_materializes_by_fetching_from_that_host() {
    // Given — a file staged on the peer, and a session about to start on A
    let env = two_daemons().await;
    stage_on_peer(&env.service_a, "remote-spec.md", b"# staged on the peer").await;

    // When — A starts a session referencing the peer's staged file
    // 60s: one StartSession here provisions a git worktree *and* fetches the attachment across two
    // hosts, both bounded by their own deadlines — this only keeps a stall from hanging the suite.
    let session_id = tokio::time::timeout(
        Duration::from_secs(60),
        env.service_a
            .start_session(Request::new(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_type: "workspace".to_string(),
                project_id: TEST_PROJECT_ID.to_string(),
                attachments: vec![SessionAttachment {
                    basename: "remote-spec.md".to_string(),
                    source: Some(AttachmentSource::Staged(StagedAttachmentRef {
                        daemon_instance_id: PEER_INSTANCE_ID.to_string(),
                        staging_id: STAGING_ID.to_string(),
                        file_name: "remote-spec.md".to_string(),
                    })),
                }],
                ..Default::default()
            })),
    )
    .await
    .expect("StartSession must return")
    .expect("a cross-host staged ref must be materialized, not refused")
    .into_inner()
    .session_id;

    // Then — the peer's bytes are in the session started on A
    let materialized = unified_session_dir_path(&env.local_base, &session_id)
        .join("artifacts")
        .join("attachments")
        .join("remote-spec.md");
    assert!(
        materialized.exists(),
        "the peer's staged bytes must be materialized at {materialized:?}"
    );
    assert_eq!(
        std::fs::read(&materialized).unwrap(),
        b"# staged on the peer"
    );
}

/// A staged file the peer never finished receiving must not be fetched across hosts. The
/// completeness marker is checked on the **owning** host, so the session host cannot be handed a
/// truncated document that it would then write as a whole attachment.
#[tokio::test]
#[serial]
async fn a_cross_host_staged_ref_whose_upload_never_completed_is_refused_and_writes_nothing() {
    // Given — a chunk staged on the peer that was never finalized
    let env = two_daemons().await;
    env.service_a
        .upload_staged_attachment_chunk(Request::new(UploadStagedAttachmentChunkRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: PEER_INSTANCE_ID.to_string(),
            staging_id: STAGING_ID.to_string(),
            file_name: "half.md".to_string(),
            data: b"first half".to_vec(),
            last: false,
        }))
        .await
        .expect("the first chunk must be accepted by the peer");

    // When — A starts a session referencing the unfinished staged file
    // 60s: same budget as the successful cross-host start above — the refusal has to travel to the
    // owning host and back before it can be observed.
    let outcome = tokio::time::timeout(
        Duration::from_secs(60),
        env.service_a
            .start_session(Request::new(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_type: "workspace".to_string(),
                project_id: TEST_PROJECT_ID.to_string(),
                attachments: vec![SessionAttachment {
                    basename: "half.md".to_string(),
                    source: Some(AttachmentSource::Staged(StagedAttachmentRef {
                        daemon_instance_id: PEER_INSTANCE_ID.to_string(),
                        staging_id: STAGING_ID.to_string(),
                        file_name: "half.md".to_string(),
                    })),
                }],
                ..Default::default()
            })),
    )
    .await
    .expect("StartSession must return");

    // Then — refused, and no attachment was written on A
    assert!(
        outcome.is_err(),
        "an incomplete cross-host staged upload must be refused"
    );
    assert!(
        !any_attachment_written(&env.local_base),
        "a refused cross-host fetch must leave no attachment behind"
    );
}

// ---------------------------------------------------------------------------
// Forwarded streaming read
// ---------------------------------------------------------------------------

/// `StreamReadHostDocument` is server-streaming, and streaming RPCs are refused for
/// `PeerRoute::Forward` today. It must forward — a document larger than the unary cap is exactly
/// the case that has no other way across hosts.
#[tokio::test]
#[serial]
async fn stream_read_host_document_forwards_to_the_peer_that_owns_the_document() {
    // Given — a document on the peer, past the unary cap so only the stream can carry it
    let env = two_daemons().await;
    let size = tddy_daemon::host_documents::MAX_HOST_DOCUMENT_BYTES + 512 * 1024;
    let document: Vec<u8> = (0..size).map(|i| (i % 241) as u8).collect();
    stage_on_peer(&env.service_a, "big-remote.bin", &document).await;

    // When — A opens the streaming read against the peer
    // 30s: opening a forwarded stream is bounded by `PEER_FORWARD_TIMEOUT`; this wait exists so a
    // missing deadline fails the test instead of hanging it.
    let mut stream = tokio::time::timeout(
        Duration::from_secs(30),
        env.service_a
            .stream_read_host_document(Request::new(peer_staged_document_request(&format!(
                "{STAGING_ID}/big-remote.bin"
            )))),
    )
    .await
    .expect("opening the forwarded stream must not hang")
    .expect("a forwarded StreamReadHostDocument must be accepted")
    .into_inner();
    let bytes = drain_document(&mut stream).await;

    // Then — every byte crossed the hosts, in order
    assert_eq!(bytes.len(), document.len(), "forwarded document truncated");
    assert_eq!(bytes, document, "forwarded document bytes differ");
}

// ---------------------------------------------------------------------------
// Forwarded streaming start-session
// ---------------------------------------------------------------------------

/// A session started on a **peer** host still reports attachment progress: `StreamStartSession` is
/// forwarded to the owning daemon and its events relayed back. Without the forward, progress is lost
/// exactly where it matters most — bytes crossing two hosts is the slowest case there is.
#[tokio::test]
#[serial]
async fn stream_start_session_forwards_to_the_peer_that_runs_the_session() {
    // Given — two daemons, and a file staged on the peer that will run the session
    let env = two_daemons().await;
    stage_on_peer(&env.service_a, "peer-spec.md", b"# runs on the peer").await;

    // When — A starts a session addressed to the peer, over the streaming RPC
    // 30s: as above, the forward's own deadline for opening the stream.
    let mut stream = tokio::time::timeout(
        Duration::from_secs(30),
        env.service_a
            .stream_start_session(Request::new(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_type: "workspace".to_string(),
                project_id: TEST_PROJECT_ID.to_string(),
                daemon_instance_id: PEER_INSTANCE_ID.to_string(),
                attachments: vec![SessionAttachment {
                    basename: "peer-spec.md".to_string(),
                    source: Some(AttachmentSource::Staged(StagedAttachmentRef {
                        daemon_instance_id: PEER_INSTANCE_ID.to_string(),
                        staging_id: STAGING_ID.to_string(),
                        file_name: "peer-spec.md".to_string(),
                    })),
                }],
                ..Default::default()
            })),
    )
    .await
    .expect("opening the forwarded start-session stream must not hang")
    .expect("a forwarded StreamStartSession must be accepted")
    .into_inner();

    let mut progressed: Vec<String> = Vec::new();
    let mut results: Vec<String> = Vec::new();
    // 30s per event: this is the relay's own idle deadline, so anything slower would have been
    // delivered as an error by the stream itself rather than as a timeout here.
    while let Some(item) = tokio::time::timeout(Duration::from_secs(30), stream.next())
        .await
        .expect("no forwarded start-session event arrived within the timeout")
    {
        let event = item.expect("forwarded StreamStartSession yielded an error");
        match event.event.expect("every event must carry a variant") {
            StartEvent::AttachmentProgress(progress) => progressed.push(progress.basename),
            StartEvent::Result(result) => results.push(result.session_id),
        }
    }

    // Then — progress crossed the hosts, and exactly one result closed the stream
    assert_eq!(progressed, vec!["peer-spec.md".to_string()]);
    assert_eq!(results.len(), 1, "expected exactly one terminal result");

    // And — the session and its attachment live on the peer, not on A
    let materialized = unified_session_dir_path(&env.peer_sessions_base, &results[0])
        .join("artifacts")
        .join("attachments")
        .join("peer-spec.md");
    assert!(
        materialized.exists(),
        "the peer must hold the session's attachment at {materialized:?}"
    );
    assert_eq!(std::fs::read(&materialized).unwrap(), b"# runs on the peer");
}

// ---------------------------------------------------------------------------
// Cross-host staged attachments past the unary ceiling
// ---------------------------------------------------------------------------

/// A cross-host staged attachment is bounded by the host's configured `max_attachment_bytes`, not by
/// the unary `MAX_HOST_DOCUMENT_BYTES`. Otherwise a document attaches fine on one host and fails
/// across two — inverting the reason the streaming path exists at all.
#[tokio::test]
#[serial]
async fn a_cross_host_staged_attachment_larger_than_the_unary_cap_is_materialized() {
    // Given — a document on the peer, past the unary ceiling but well under the configured cap
    let env = two_daemons().await;
    let size = tddy_daemon::host_documents::MAX_HOST_DOCUMENT_BYTES + 512 * 1024;
    let document: Vec<u8> = (0..size).map(|i| (i % 239) as u8).collect();
    stage_on_peer(&env.service_a, "big-attach.bin", &document).await;

    // When — A starts a session referencing the peer's staged document
    // 120s: a 4.5 MiB document crosses the LiveKit data channel in ~100 chunk-framed messages on top
    // of provisioning a git worktree, and the link's throughput is the machine's, not the test's.
    let session_id = tokio::time::timeout(
        Duration::from_secs(120),
        env.service_a
            .start_session(Request::new(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_type: "workspace".to_string(),
                project_id: TEST_PROJECT_ID.to_string(),
                attachments: vec![SessionAttachment {
                    basename: "big-attach.bin".to_string(),
                    source: Some(AttachmentSource::Staged(StagedAttachmentRef {
                        daemon_instance_id: PEER_INSTANCE_ID.to_string(),
                        staging_id: STAGING_ID.to_string(),
                        file_name: "big-attach.bin".to_string(),
                    })),
                }],
                ..Default::default()
            })),
    )
    .await
    .expect("StartSession must return")
    .expect("an over-unary-cap cross-host staged attachment must be materialized")
    .into_inner()
    .session_id;

    // Then — every byte crossed the hosts and landed in the session started on A
    let materialized = unified_session_dir_path(&env.local_base, &session_id)
        .join("artifacts")
        .join("attachments")
        .join("big-attach.bin");
    let written = std::fs::read(&materialized).expect("the attachment must exist");
    assert_eq!(written.len(), document.len(), "attachment was truncated");
    assert_eq!(written, document, "attachment bytes differ from the source");
}

// ---------------------------------------------------------------------------
// The primary multi-host flow: staged here, session there
// ---------------------------------------------------------------------------

/// The feature's primary multi-host flow: the browser stages its bytes on the daemon it is connected
/// to (A) and starts the session on another host (B). B's materialization **is** the cross-host
/// transfer, so progress must advance while the bytes are moving.
///
/// Reporting only once the whole attachment has landed makes the gap between two relayed frames the
/// entire transfer, which leaves `PEER_FORWARD_STREAM_IDLE_TIMEOUT` covering a whole file: an
/// ordinary document then fails with `DEADLINE_EXCEEDED` while transferring perfectly well. It also
/// pins the row's progress bar at 0% until it jumps to done, which is the point of streaming at all.
///
/// The sibling `stream_start_session_forwards_to_the_peer_that_runs_the_session` stages **on** the
/// session host, so its materialization takes the local-copy path and never crosses a host.
#[tokio::test]
#[serial]
async fn stream_start_session_on_the_peer_reports_progress_while_staged_bytes_cross_from_this_host()
{
    // Given — a document staged on A, past the unary ceiling so the transfer spans many frames
    let env = two_daemons().await;
    let size = tddy_daemon::host_documents::MAX_HOST_DOCUMENT_BYTES + 512 * 1024;
    let document: Vec<u8> = (0..size).map(|i| (i % 233) as u8).collect();
    let staged = stage_on_local(&env, "handbook.pdf", &document).await;
    assert!(
        staged.is_file(),
        "the document must be staged on A, the host that will not run the session: {staged:?}"
    );

    // When — A starts the session on the peer, over the streaming RPC, referencing its own staged bytes
    // 30s: matches the forward's own deadline for opening the stream (`PEER_FORWARD_TIMEOUT`), so a
    // hang fails this test instead of hanging the suite.
    let mut stream = tokio::time::timeout(
        Duration::from_secs(30),
        env.service_a
            .stream_start_session(Request::new(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_type: "workspace".to_string(),
                project_id: TEST_PROJECT_ID.to_string(),
                daemon_instance_id: PEER_INSTANCE_ID.to_string(),
                attachments: vec![SessionAttachment {
                    basename: "handbook.pdf".to_string(),
                    source: Some(AttachmentSource::Staged(StagedAttachmentRef {
                        daemon_instance_id: LOCAL_INSTANCE_ID.to_string(),
                        staging_id: STAGING_ID.to_string(),
                        file_name: "handbook.pdf".to_string(),
                    })),
                }],
                ..Default::default()
            })),
    )
    .await
    .expect("opening the forwarded start-session stream must not hang")
    .expect("a forwarded StreamStartSession must be accepted")
    .into_inner();

    let mut progress: Vec<(String, u64, u64)> = Vec::new();
    let mut results: Vec<String> = Vec::new();
    // 30s per event: this is the relay's own idle deadline, so anything slower would have been
    // delivered as an error by the stream itself rather than as a timeout here.
    while let Some(item) = tokio::time::timeout(Duration::from_secs(30), stream.next())
        .await
        .expect("no forwarded start-session event arrived within the timeout")
    {
        let event = item.expect("forwarded StreamStartSession yielded an error");
        match event.event.expect("every event must carry a variant") {
            StartEvent::AttachmentProgress(reported) => progress.push(progress_row(reported)),
            StartEvent::Result(result) => results.push(result.session_id),
        }
    }

    // Then — progress advanced one transfer frame at a time, then once more with the size on disk.
    // `attachment_index` / `attachment_count` are pinned by the single-host sibling in
    // `tests/session_attach_staging_scope_acceptance.rs`; what only a cross-host start can show is
    // `bytes_done` moving while the bytes are still in flight.
    let total = document.len() as u64;
    let frame = tddy_daemon::connection_service::HOST_DOCUMENT_FRAME_BYTES as u64;
    let expected: Vec<(String, u64, u64)> = (1..=total.div_ceil(frame))
        .map(|nth| (nth * frame).min(total))
        .chain(std::iter::once(total))
        .map(|bytes_done| ("handbook.pdf".to_string(), bytes_done, total))
        .collect();
    assert_eq!(progress, expected);

    // And — exactly one result closed the stream
    assert_eq!(results.len(), 1, "expected exactly one terminal result");

    // And — the peer holds the bytes verbatim
    let materialized = unified_session_dir_path(&env.peer_sessions_base, &results[0])
        .join("artifacts")
        .join("attachments")
        .join("handbook.pdf");
    let written = std::fs::read(&materialized).expect("the peer must hold the attachment");
    assert_eq!(written.len(), document.len(), "attachment was truncated");
    assert_eq!(written, document, "attachment bytes differ from the source");
}
