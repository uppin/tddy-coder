//! Acceptance: multi-host forwarding of the staging + `ReadHostDocument` RPCs.
//!
//! PRD: `docs/ft/coder/session-attachments.md` § Start-session materialization (AC6, AC8).
//!
//! Two daemons share a common room (the in-memory `tddy-room-testkit` hub). A staging RPC (or a
//! `ReadHostDocument` triggered by `StartSession`) addressed to a peer `daemon_instance_id` is
//! forwarded to that peer and operates on the **peer's** staging root / data root. They are
//! `#[serial]` because they share staging roots on disk.
//!
//! Single-daemon materialization behavior lives in `tests/staging_rpc_acceptance.rs`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use prost::Message as _;
use serial_test::serial;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_livekit::livekit_peer_discovery::{AdvertisedSigningKey, CommonRoomPeerRegistry};
use tddy_host_service::peer_routing::PeerRouting;
use tddy_room_testkit::{HubEligibleDaemonSource, HubParticipant, HubPeerForwarder, RoomHub};
use tddy_rpc::{Request, RpcMessage, RpcResult, RpcService as _};
use tddy_service::proto::session::{
    session_attachment::Source as AttachmentSource, HostDocumentRef, SessionAttachment,
    SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_service::proto::session_files::UploadStagedAttachmentChunkRequest;
use tddy_service::proto::types::HostDocumentScope;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::{wait_until_peer_discovered, TEST_TOKEN};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The hub is fresh per test, so the room name needs no uniqueness.
const FORWARDING_ROOM: &str = "attach-start-forwarding-room";
const PEER_INSTANCE_ID: &str = "attach-start-peer-daemon";
const TEST_PROJECT_ID: &str = "attach-start-fwd-proj";
const STAGING_ID: &str = "ffffffff-ffff-7fff-8fff-ffffffffffff";

/// The identity a daemon serves `the pre-unbundle monolithic RPC coordinate` on — a fixed `daemon-` prefix over
/// its instance id, the way `main.rs` joins the common room. The bare instance id belongs to the
/// discovery participant, which publishes the advertisement and serves no RPC.
/// See `docs/ft/web/daemon-selector-livekit-rpc.md`.
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

fn write_daemon_yaml(
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
{id_block}allowed_tools:
  - path: {true_path}
    label: t
livekit:
  enabled: true
  url: ws://127.0.0.1:7880
  api_key: devkey
  api_secret: secret
  common_room: {FORWARDING_ROOM}
"#
    );
    std::fs::write(&path, yaml).unwrap();
    std::fs::write(
        dir.path().join("users.yaml"),
        format!("- github_user: \"testuser\"\n  os_user: \"{os_user}\"\n"),
    )
    .unwrap();
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
    service_a: Arc<DaemonSessionHost>,
    peer_base: PathBuf,
    /// The peer's staging base — its own root, separate from its data dir since staging moved to a
    /// restart-cleared location.
    peer_staging_base: PathBuf,
    local_base: PathBuf,
    _peer_rpc: HubParticipant,
    _peer_advertisement: HubParticipant,
    _attach_a: tokio::task::JoinHandle<()>,
    // Keep-alive guards — held until the struct is dropped.
    _repo: tempfile::TempDir,
    _config_a: tempfile::TempDir,
    _config_b: tempfile::TempDir,
    _sessions_a: tempfile::TempDir,
    _sessions_b: tempfile::TempDir,
    _staging_a: tempfile::TempDir,
    _staging_b: tempfile::TempDir,
}

async fn two_daemons() -> TwoDaemons {
    let hub = RoomHub::new();
    let os_user = std::env::var("USER").expect("USER required");

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let (config_a_dir, path_a) = write_daemon_yaml(None, &os_user);
    let (config_b_dir, path_b) = write_daemon_yaml(Some(PEER_INSTANCE_ID), &os_user);
    let config_a = DaemonConfig::load(&path_a).unwrap();
    let config_b = DaemonConfig::load(&path_b).unwrap();

    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));

    // Peer daemon B: registered project + service, joined to the common room as an RPC participant.
    let sessions_b = tempfile::tempdir().unwrap();
    let staging_b = tempfile::tempdir().unwrap();
    register_project(&sessions_b.path().join("projects"), repo_dir.path());
    let base_b = sessions_b.path().to_path_buf();
    let resolver_b: SessionsBaseResolver = Arc::new(move |_| Some(base_b.clone()));
    let service_b = DaemonSessionHost::new(
        config_b.clone(),
        resolver_b,
        sessions_b.path().to_path_buf(),
        user_resolver.clone(),
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config_b),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(tddy_session_lifecycle::claude_cli_session::CliSessionManager::new()),
        tddy_session_lifecycle::test_util::builtin_coding_agents(),
    )
    .with_staging_base_dir(staging_b.path().to_path_buf());

    // B's discovery participant: bare instance id, publishes the advertisement, serves no RPC.
    let peer_advertisement =
        hub.advertise_daemon(FORWARDING_ROOM, &config_b, &AdvertisedSigningKey::default());

    // B's RPC participant: `daemon-{instance_id}`, the identity a forward must address.
    // Both coordinates, because a forward is addressed at the service that *declares* the method:
    // `StartSession` is still `the pre-unbundle monolithic RPC coordinate`'s, while the staging and host-document
    // RPCs became `session_files.SessionFilesService`'s with `#unbundle` node 6.
    let (service_b, families_b) = tddy_daemon_rpc::RpcHandlers::install(service_b);
    let service_b = Arc::new(service_b);
    let server = tddy_rpc::MultiRpcService::new(
        [
            service_b.session_files_entry(),
            service_b.session_lifecycle_entry(),
        ]
        .into_iter()
        .chain(families_b.entries())
        .collect(),
    );
    let peer_rpc = hub.join(FORWARDING_ROOM, &rpc_identity(PEER_INSTANCE_ID), "");
    peer_rpc.serve(Arc::new(server));

    // Local daemon A: discovery wired so it can route to the peer.
    let sessions_a = tempfile::tempdir().unwrap();
    let staging_a = tempfile::tempdir().unwrap();
    register_project(&sessions_a.path().join("projects"), repo_dir.path());
    let base_a = sessions_a.path().to_path_buf();
    let resolver_a: SessionsBaseResolver = Arc::new(move |_| Some(base_a.clone()));
    let registry = Arc::new(CommonRoomPeerRegistry::new());
    let attach_a = hub.attach_registry(
        FORWARDING_ROOM,
        &local_instance_id_for_config(&config_a),
        registry.clone(),
    );
    let common_room_a = CommonRoom::new(
        HubPeerForwarder::new(
            hub.clone(),
            FORWARDING_ROOM,
            rpc_identity(&local_instance_id_for_config(&config_a)),
        ),
        &config_a,
    );
    let eligible: Arc<dyn tddy_host_service::multi_host::EligibleDaemonSource> =
        HubEligibleDaemonSource::new(Arc::new(config_a.clone()), registry, common_room_a.clone());
    let service_a = DaemonSessionHost::new(
        config_a.clone(),
        resolver_a,
        sessions_a.path().to_path_buf(),
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        PeerRouting::new(config_a.clone(), eligible, Some(common_room_a)),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(tddy_session_lifecycle::claude_cli_session::CliSessionManager::new()),
        tddy_session_lifecycle::test_util::builtin_coding_agents(),
    )
    .with_staging_base_dir(staging_a.path().to_path_buf());

    // Wait until A discovers B, asked through `host.HostService` against A's own roster.
    wait_until_peer_discovered(
        &service_a,
        TEST_TOKEN,
        PEER_INSTANCE_ID,
        Duration::from_secs(45),
    )
    .await;

    let base_a = sessions_a.path().to_path_buf();
    let peer_base = sessions_b.path().to_path_buf();
    TwoDaemons {
        service_a: Arc::new(service_a),
        peer_base,
        peer_staging_base: staging_b.path().to_path_buf(),
        local_base: base_a,
        _peer_rpc: peer_rpc,
        _peer_advertisement: peer_advertisement,
        _attach_a: attach_a,
        _repo: repo_dir,
        _config_a: config_a_dir,
        _config_b: config_b_dir,
        _sessions_a: sessions_a,
        _sessions_b: sessions_b,
        _staging_a: staging_a,
        _staging_b: staging_b,
    }
}

/// AC6 — `UploadStagedAttachmentChunk` addressed to a peer `daemon_instance_id` is forwarded
/// and writes the file into the **peer's** staging root.
#[tokio::test]
#[serial]
async fn staging_rpcs_addressed_to_a_peer_daemon_forward_and_operate_on_the_peer_staging_root() {
    // Given — two daemons in a common room
    let env = two_daemons().await;
    let peer_staging_base = env.peer_staging_base.clone();
    // The registered entry, not the bare service: routing is the wrapper's, so a test about
    // forwarding that addressed the crate's implementation directly would prove nothing.
    let staging_on_a = env.service_a.session_files_entry();

    // When — A stages a file addressed to the peer
    let request = UploadStagedAttachmentChunkRequest {
        session_token: TEST_TOKEN.to_string(),
        daemon_instance_id: PEER_INSTANCE_ID.to_string(),
        staging_id: STAGING_ID.to_string(),
        file_name: "remote.md".to_string(),
        data: b"staged on peer".to_vec(),
        last: true,
    };
    let answer = staging_on_a
        .service
        .handle_rpc(
            staging_on_a.name,
            "UploadStagedAttachmentChunk",
            &RpcMessage::new(
                request.encode_to_vec(),
                tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
            ),
        )
        .await;
    match answer {
        RpcResult::Unary(Ok(_)) => {}
        RpcResult::Unary(Err(status)) => {
            panic!(
                "forwarded staging upload must succeed: {}",
                status.message()
            )
        }
        RpcResult::ServerStream(_) => panic!("UploadStagedAttachmentChunk is a unary RPC"),
    }

    // Then — the file landed on the peer's staging root, not A's
    let os_user = std::env::var("USER").unwrap();
    let peer_staged = tddy_session_files::session_attachment_staging::staging_root_for(
        &os_user,
        &peer_staging_base,
    )
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
        .start_session(Request::direct(StartSessionRequest {
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
        .start_session(Request::direct(StartSessionRequest {
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
