//! Acceptance: multi-host forwarding of the staging + `ReadHostDocument` RPCs.
//!
//! PRD: `docs/ft/coder/amendments/session-attachments-start-materialization.md` (AC6, AC8).
//!
//! Two daemons share a LiveKit common room. A staging RPC (or a `ReadHostDocument` triggered
//! by `StartSession`) addressed to a peer `daemon_instance_id` is forwarded to that peer and
//! operates on the **peer's** staging root / data root. These need the LiveKit testkit container
//! (Docker or `LIVEKIT_TESTKIT_WS_URL`); they are `#[serial]` so they own the container alone.
//!
//! Single-daemon materialization behavior lives in `tests/staging_rpc_acceptance.rs`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::RoomOptions;
use serial_test::serial;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::{
    spawn_common_room_discovery_task, CommonRoomPeerRegistry, DaemonAdvertisement,
    LiveKitDiscoveryHandles, LiveKitEligibleDaemonSource,
};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_livekit::LiveKitParticipant;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    session_attachment::Source as AttachmentSource, ConnectionService as ConnectionServiceTrait,
    HostDocumentRef, HostDocumentScope, ListEligibleDaemonsRequest, SessionAttachment,
    StartSessionRequest, UploadStagedAttachmentChunkRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const ROOM: &str = "attach-start-forwarding-room";
const PEER_INSTANCE_ID: &str = "attach-start-peer-daemon";
const LK_API_KEY: &str = "devkey";
const LK_API_SECRET: &str = "secret";
const TEST_PROJECT_ID: &str = "attach-start-fwd-proj";
const STAGING_ID: &str = "ffffffff-ffff-7fff-8fff-ffffffffffff";

fn true_bin() -> &'static str {
    if cfg!(target_os = "macos") {
        "/usr/bin/true"
    } else {
        "/bin/true"
    }
}

fn write_livekit_daemon_yaml(
    ws_url: &str,
    daemon_instance_id: Option<&str>,
    os_user: &str,
) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    let id_block = daemon_instance_id
        .map(|id| format!("daemon_instance_id: {id}\n"))
        .unwrap_or_default();
    let true_path = true_bin();
    let yaml = format!(
        r#"
{id_block}users:
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

fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: fwd-proj\n    git_url: \"\"\n    main_repo_path: {}\n",
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn create_test_repo_with_origin(dir: &std::path::Path) {
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

/// Brings up peer daemon B in the common room and local daemon A with discovery wired to it.
/// Owns every `TempDir` backing the two daemons (sessions roots, config dirs, the shared repo) so
/// they survive for the test body — `projects.yaml` and the worktree origin must still exist when
/// the test runs. Dropping this struct at test end tears everything down.
struct TwoDaemons {
    service_a: ConnectionServiceImpl,
    peer_base: PathBuf,
    local_base: PathBuf,
    _peer_run: tokio::task::JoinHandle<()>,
    _livekit: LiveKitTestkit,
    // Keep-alive guards — held until the struct is dropped.
    _repo: tempfile::TempDir,
    _config_a: tempfile::TempDir,
    _config_b: tempfile::TempDir,
    _sessions_a: tempfile::TempDir,
    _sessions_b: tempfile::TempDir,
}

async fn two_daemons() -> TwoDaemons {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let ws_url = livekit.get_ws_url();
    let os_user = std::env::var("USER").expect("USER required");

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let (config_a_dir, path_a) = write_livekit_daemon_yaml(&ws_url, None, &os_user);
    let (config_b_dir, path_b) =
        write_livekit_daemon_yaml(&ws_url, Some(PEER_INSTANCE_ID), &os_user);
    let config_a = DaemonConfig::load(&path_a).unwrap();
    let config_b = DaemonConfig::load(&path_b).unwrap();

    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));

    // Peer daemon B: registered project + service, joined to the common room as an RPC participant.
    let sessions_b = tempfile::tempdir().unwrap();
    register_project(&sessions_b.path().join("projects"), repo_dir.path());
    let base_b = sessions_b.path().to_path_buf();
    let resolver_b: SessionsBaseResolver = Arc::new(move |_| Some(base_b.clone()));
    let service_b = ConnectionServiceImpl::new(
        config_b,
        resolver_b,
        sessions_b.path().to_path_buf(),
        user_resolver.clone(),
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    );
    let token_b = livekit
        .generate_token(ROOM, PEER_INSTANCE_ID)
        .expect("LiveKit token for peer");
    let server = tddy_service::ConnectionServiceServer::new(service_b);
    let participant = LiveKitParticipant::connect(
        &ws_url,
        &token_b,
        server,
        RoomOptions::default(),
        None,
        None,
    )
    .await
    .expect("peer daemon joins common room");
    let adv = DaemonAdvertisement {
        instance_id: PEER_INSTANCE_ID.to_string(),
        label: PEER_INSTANCE_ID.to_string(),
        repos_base_path: String::new(),
    };
    participant
        .room()
        .local_participant()
        .set_metadata(serde_json::to_string(&adv).unwrap())
        .await
        .expect("peer publishes advertisement");
    let peer_run = tokio::spawn(async move { participant.run().await });

    // Local daemon A: discovery wired so it can route to the peer.
    let sessions_a = tempfile::tempdir().unwrap();
    register_project(&sessions_a.path().join("projects"), repo_dir.path());
    let base_a = sessions_a.path().to_path_buf();
    let resolver_a: SessionsBaseResolver = Arc::new(move |_| Some(base_a.clone()));
    let config_arc = Arc::new(config_a.clone());
    let registry = Arc::new(CommonRoomPeerRegistry::new());
    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
    spawn_common_room_discovery_task(config_arc.clone(), registry.clone(), room_slot.clone());
    let eligible: Arc<dyn tddy_daemon::multi_host::EligibleDaemonSource> = Arc::new(
        LiveKitEligibleDaemonSource::new(config_arc, registry, room_slot.clone()),
    );
    let service_a = ConnectionServiceImpl::new(
        config_a,
        resolver_a,
        sessions_a.path().to_path_buf(),
        user_resolver,
        None,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: eligible,
            common_room_livekit_room: room_slot,
        }),
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    );

    // Wait until A discovers B.
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            let daemons = service_a
                .list_eligible_daemons(Request::new(ListEligibleDaemonsRequest {
                    session_token: TEST_TOKEN.to_string(),
                }))
                .await
                .expect("ListEligibleDaemons")
                .into_inner()
                .daemons;
            if daemons.iter().any(|d| d.instance_id == PEER_INSTANCE_ID) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .expect("timeout waiting for peer daemon in eligible list");

    let base_a = sessions_a.path().to_path_buf();
    let peer_base = sessions_b.path().to_path_buf();
    TwoDaemons {
        service_a,
        peer_base,
        local_base: base_a,
        _peer_run: peer_run,
        _livekit: livekit,
        _repo: repo_dir,
        _config_a: config_a_dir,
        _config_b: config_b_dir,
        _sessions_a: sessions_a,
        _sessions_b: sessions_b,
    }
}

/// AC6 — `UploadStagedAttachmentChunk` addressed to a peer `daemon_instance_id` is forwarded
/// and writes the file into the **peer's** staging root.
#[tokio::test]
#[serial]
async fn staging_rpcs_addressed_to_a_peer_daemon_forward_and_operate_on_the_peer_staging_root() {
    // Given — two daemons in a common room
    let env = two_daemons().await;
    let service_a = &env.service_a;
    let peer_base = env.peer_base.clone();

    // When — A stages a file addressed to the peer
    service_a
        .upload_staged_attachment_chunk(Request::new(UploadStagedAttachmentChunkRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: PEER_INSTANCE_ID.to_string(),
            staging_id: STAGING_ID.to_string(),
            file_name: "remote.md".to_string(),
            data: b"staged on peer".to_vec(),
            last: true,
        }))
        .await
        .expect("forwarded staging upload must succeed");

    // Then — the file landed on the peer's staging root, not A's
    let os_user = std::env::var("USER").unwrap();
    let peer_staged =
        tddy_daemon::session_attachment_staging::staging_root_for(&os_user, &peer_base)
            .join(STAGING_ID)
            .join("remote.md");
    assert!(
        peer_staged.exists(),
        "staged file must land on the peer at {peer_staged:?}"
    );
    assert_eq!(std::fs::read(&peer_staged).unwrap(), b"staged on peer");
}

/// AC8 — a `HostDocumentRef` naming a peer `daemon_instance_id` triggers a `ReadHostDocument`
/// forward to that peer; the bytes land in the local session's `artifacts/attachments/`.
#[tokio::test]
#[serial]
async fn a_host_document_ref_naming_a_peer_daemon_is_forwarded_via_read_host_document() {
    // Given — two daemons; the peer holds a session with an artifacts/PRD.md
    let env = two_daemons().await;
    let service_a = &env.service_a;
    let peer_base = env.peer_base.clone();
    let local_base = env.local_base.clone();

    // Start a session on the peer so it owns an artifact. (Routed to the peer via daemon_instance_id.)
    let peer_session = service_a
        .start_session(Request::new(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_type: "workspace".to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            daemon_instance_id: PEER_INSTANCE_ID.to_string(),
            ..Default::default()
        }))
        .await
        .expect("StartSession on the peer must succeed")
        .into_inner()
        .session_id;
    let peer_dir = unified_session_dir_path(&peer_base, &peer_session);
    std::fs::create_dir_all(peer_dir.join("artifacts")).unwrap();
    std::fs::write(peer_dir.join("artifacts").join("PRD.md"), b"peer-owned doc").unwrap();

    // When — A starts a local session referencing the peer's PRD.md as a host document
    let local_session = service_a
        .start_session(Request::new(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_type: "workspace".to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            attachments: vec![SessionAttachment {
                basename: "PRD.md".to_string(),
                source: Some(AttachmentSource::HostDocument(HostDocumentRef {
                    daemon_instance_id: PEER_INSTANCE_ID.to_string(),
                    scope: HostDocumentScope::SessionArtifact.into(),
                    session_id: peer_session.clone(),
                    project_id: String::new(),
                    relative_path: "PRD.md".to_string(),
                })),
            }],
            ..Default::default()
        }))
        .await
        .expect("StartSession with a peer host document must succeed")
        .into_inner()
        .session_id;

    // Then — the peer's doc bytes were fetched and copied into the local session's attachments
    let local_dir = unified_session_dir_path(&local_base, &local_session);
    let copied = local_dir
        .join("artifacts")
        .join("attachments")
        .join("PRD.md");
    assert!(
        copied.exists(),
        "peer host document must be copied into local attachments at {copied:?}"
    );
    assert_eq!(std::fs::read(&copied).unwrap(), b"peer-owned doc");
}
