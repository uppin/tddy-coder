//! The daemon's Unix-domain socket, and the six services mounted on it.
//!
//! Two things are only true over this transport. `MintLocalToken` is the first: the socket is the
//! only place a caller's SO_PEERCRED uid is available, so minting is exercised end to end here — a
//! peer uid mapped to a configured user gets an access token the shared signer verifies, an
//! unmapped uid is denied.
//!
//! The second is that `host.HostService` and `worktree.WorktreeService` reach their implementations
//! at all. Both are served through **hand-written** tonic adapters
//! (`host_tonic_adapter.rs`, `worktree_tonic_adapter.rs`), written before `tddy-codegen`'s
//! `generate_tonic_adapter` existed, so all 17 delegations are spelled out by hand. A method wired
//! to the wrong inner call, or a stream arm that drops the error mapping, compiles and ships.
//! Nothing but a call over the wire catches that, so each adapter is exercised here through a real
//! client: one unary method, one server-streaming method, and the refusal a streaming method must
//! propagate rather than swallow. The later families' adapters — the terminal one from node 6 and
//! node 7's session-agent and activity pair — are generated, so a mis-wired delegation is not the
//! hazard there; what is, is the mount, and that is asserted the same way (see the last section).
//!
//! The third is that `terminal_session.TerminalSessionService` is reachable here at all. The in-jail
//! `tddy-sandbox-app` has no transport but this socket, and its whole terminal bridge is the bidi
//! `StreamSessionTerminalIO`. A coordinate left off this builder still answers over LiveKit and
//! HTTP — and nowhere a jail can reach, which is every sandboxed session's terminal lost in
//! silence. So the mount is asserted through a call over the wire, not by reading the builder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hyper_util::rt::TokioIo;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_tonic_adapter::{ConnectionServiceTonicAdapter, UidToUsername};
use tddy_daemon::host_tonic_adapter::HostServiceTonicAdapter;
use tddy_daemon::local_socket_server::{serve_connection_uds, LocalSocketServices};
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_daemon::user_sessions_path::username_for_uid;
use tddy_daemon::worktree_tonic_adapter::WorktreeServiceTonicAdapter;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_github::{SessionTokenSigner, TokenKind};
use tddy_service::proto::activity::{ActivityServiceTonicAdapter, ReportSessionStatusRequest};
use tddy_service::proto::catalog::CatalogServiceTonicAdapter;
use tddy_service::proto::connection::MintLocalTokenRequest;
use tddy_service::proto::exec_tools::ExecToolServiceTonicAdapter;
use tddy_service::proto::pr_stack::PrStackServiceTonicAdapter;
use tddy_service::proto::host::{ListEligibleDaemonsRequest, StreamHostStatsRequest};
use tddy_service::proto::session_agents_svc::{
    ListSessionAgentsRequest, SessionAgentServiceTonicAdapter,
};
use tddy_service::proto::tonic_activity::activity_service_client::ActivityServiceClient;
use tddy_service::proto::tonic_session_agents::session_agent_service_client::SessionAgentServiceClient;
use tddy_service::proto::worktree::{
    ListWorktreesForProjectRequest, StreamWorktreeStatsRequest, WorktreeRow,
};
use tddy_service::tonic_connection::connection_service_client::ConnectionServiceClient;
use tddy_service::tonic_host::host_service_client::HostServiceClient;
use tddy_service::tonic_worktree::worktree_service_client::WorktreeServiceClient;
use tddy_terminal_rpc::proto::terminal_session::{
    ClaimTerminalControlRequest, SessionTerminalInput, TerminalSessionServiceTonicAdapter,
};
use tddy_terminal_rpc::proto::tonic_terminal_session::terminal_session_service_client::TerminalSessionServiceClient;
use tddy_worktree_service::project_storage::{self, ProjectData};
use tonic::transport::{Channel, Endpoint};

/// How long a stream is given to produce its first frame before the test fails rather than hangs.
const A_FRAME_ARRIVES_WITHIN: Duration = Duration::from_secs(5);

const TEST_SECRET: &[u8] = b"local-socket-test-secret";

/// The OS username the test process runs as — a real, passwd-resolvable name, obtained through the
/// very lookup the production adapter injects, so the peer uid over the loopback socket maps back
/// to it.
fn current_username() -> String {
    let uid = unsafe { libc::getuid() };
    username_for_uid(uid).expect("current uid resolves to a username")
}

fn a_daemon_config_mapping(os_user: &str, github_login: &str) -> DaemonConfig {
    let yaml = format!("users:\n  - github_user: \"{github_login}\"\n    os_user: \"{os_user}\"\n");
    serde_yaml::from_str(&yaml).expect("parse daemon config")
}

/// The sessions base every implementation served on this socket is rooted at, inside the socket's
/// own tempdir. Named once so a fixture written for a served service lands where it looks.
fn sessions_base_under(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions")
}

/// Start the UDS `ConnectionService` on a fresh tempdir socket. Returns the socket path plus the
/// tempdir guard (kept alive by the caller) and the shutdown sender (drop to stop the server).
fn start_local_socket_server(
    config: DaemonConfig,
    signer: Option<SessionTokenSigner>,
) -> (PathBuf, tempfile::TempDir, tokio::sync::oneshot::Sender<()>) {
    let dir = tempfile::tempdir().expect("create socket tempdir");
    let socket_path = dir.path().join("tddy-daemon.sock");
    let sessions_base = sessions_base_under(dir.path());
    std::fs::create_dir_all(&sessions_base).expect("create sessions base");

    let uid_to_username: UidToUsername = Arc::new(username_for_uid);
    let connection = Arc::new(test_service(sessions_base));
    let adapter = ConnectionServiceTonicAdapter::new(
        Arc::clone(&connection),
        Arc::new(config.clone()),
        signer,
        uid_to_username,
    );
    // The terminal coordinate is built from the *same* `ConnectionServiceImpl` the socket's
    // `ConnectionService` is, so it addresses that instance's terminals and control lease — the
    // wiring `runtime::build` does, rather than a second set of managers only this suite would see.
    let terminal_adapter =
        TerminalSessionServiceTonicAdapter::new(Arc::new(connection.terminal_session_service()));
    // The same socket carries the host and worktree services (`#unbundle` node 1), each behind its
    // own hand-written tonic adapter. Both are rooted at this tempdir, so a fixture written under it —
    // see `a_project_under` — is a project the served implementation actually finds.
    //
    // Their user resolvers are the moved crates' own `TEST_TOKEN` ones rather than the caller's
    // `config`: `MintLocalToken` authenticates by peer uid and these two by session token, and a
    // worktree service mapped to the *daemon* config would refuse every call before reaching the
    // handler under test.
    let host_adapter = HostServiceTonicAdapter::new(Arc::new(
        tddy_host_service::test_util::test_service(dir.path()),
    ));
    let worktree_adapter =
        WorktreeServiceTonicAdapter::new(Arc::new(tddy_worktree_service::test_util::test_service(
            dir.path().to_path_buf(),
            &current_username(),
        )));

    // Node 7's two coordinates, likewise built from the same `ConnectionServiceImpl`: five of the
    // nine family-B methods are what `tddy-sandbox-runner`'s relay allowlist permits an in-jail
    // agent to reach, and this socket is the only transport a jail has.
    let session_agent_adapter =
        SessionAgentServiceTonicAdapter::new(Arc::new(connection.session_agents_service()));
    let activity_adapter =
        ActivityServiceTonicAdapter::new(Arc::new(connection.activity_service()));
    let catalog_adapter = CatalogServiceTonicAdapter::new(Arc::clone(&connection));
    let exec_tool_adapter = ExecToolServiceTonicAdapter::new(Arc::clone(&connection));
    let pr_stack_adapter = PrStackServiceTonicAdapter::new(Arc::clone(&connection));

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let serve_path = socket_path.clone();
    tokio::spawn(async move {
        let shutdown = async {
            let _ = shutdown_rx.await;
        };
        serve_connection_uds(
            &serve_path,
            LocalSocketServices {
                connection: adapter,
                host: host_adapter,
                worktree: worktree_adapter,
                terminal: terminal_adapter,
                session_agents: session_agent_adapter,
                activity: activity_adapter,
                catalog: catalog_adapter,
                exec_tools: exec_tool_adapter,
                pr_stack: pr_stack_adapter,
            },
            shutdown,
        )
        .await
        .expect("serve local socket");
    });

    (socket_path, dir, shutdown_tx)
}

/// Connect a channel to the UDS. Waits (bounded) for the server task to bind the socket — we start
/// the producer ourselves, so this only smooths the startup race.
async fn connect_channel(socket_path: &Path) -> Channel {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !socket_path.exists() {
        assert!(Instant::now() < deadline, "socket was not bound in time");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let path = socket_path.to_path_buf();
    Endpoint::try_from("http://127.0.0.1:50051")
        .expect("build endpoint")
        .connect_with_connector(tower::service_fn(move |_| {
            let path = path.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(&path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .expect("connect over local socket")
}

async fn connect_client(socket_path: &Path) -> ConnectionServiceClient<Channel> {
    ConnectionServiceClient::new(connect_channel(socket_path).await)
}

#[tokio::test]
async fn mints_an_access_token_for_the_mapped_local_peer() {
    // Given — the current OS user is mapped to a GitHub login, and a shared signer is configured
    let signer = SessionTokenSigner::new(TEST_SECRET);
    let config = a_daemon_config_mapping(&current_username(), "octocat-local");
    let (socket_path, _dir, _shutdown) = start_local_socket_server(config, Some(signer.clone()));
    let mut client = connect_client(&socket_path).await;

    // When
    let response = client
        .mint_local_token(MintLocalTokenRequest {})
        .await
        .expect("mint local token")
        .into_inner();

    // Then — the token verifies to the mapped login as an access token
    let claims = signer
        .verify(&response.session_token)
        .expect("minted token verifies with the shared signer");
    assert_eq!(claims.login, "octocat-local");
    assert_eq!(claims.kind, TokenKind::Access);
}

#[tokio::test]
async fn denies_minting_for_an_unmapped_local_peer() {
    // Given — the config maps a different OS user, so the caller's peer uid resolves to no mapping
    let signer = SessionTokenSigner::new(TEST_SECRET);
    let config = a_daemon_config_mapping("someone-else", "octocat-local");
    let (socket_path, _dir, _shutdown) = start_local_socket_server(config, Some(signer));
    let mut client = connect_client(&socket_path).await;

    // When
    let status = client
        .mint_local_token(MintLocalTokenRequest {})
        .await
        .expect_err("unmapped peer must be denied");

    // Then
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

// ---------------------------------------------------------------------------
// The hand-written tonic adapters, over the same socket
// ---------------------------------------------------------------------------

/// A running socket plus the tempdir both moved services are rooted at.
struct AServedSocket {
    socket_path: PathBuf,
    dir: tempfile::TempDir,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

/// The socket the adapter suites talk to. The daemon config maps this peer, which is irrelevant to
/// the two services under test and keeps the harness to one shape.
fn a_served_socket() -> AServedSocket {
    let config = a_daemon_config_mapping(&current_username(), "octocat-local");
    let (socket_path, dir, shutdown) = start_local_socket_server(config, None);
    AServedSocket {
        socket_path,
        dir,
        _shutdown: shutdown,
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|error| panic!("git {args:?} in {cwd:?}: {error}"));
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

/// A registered project whose main repo is a real git repo carrying one secondary worktree, written
/// under the served socket's data dir so `WorktreeService` resolves it.
struct AProject {
    project_id: String,
    /// The secondary worktree's directory name, which must appear in every listing of the project.
    worktree_name: String,
    _repo_tmp: tempfile::TempDir,
}

fn a_project_under(data_dir: &Path) -> AProject {
    let os_user = current_username();
    let projects_dir = projects_path_for_user(&os_user, Some(data_dir)).expect("projects dir");

    let repo_tmp = tempfile::tempdir().expect("repo tempdir");
    let repo = repo_tmp.path().join("main");
    std::fs::create_dir_all(&repo).expect("create repo dir");
    git(&repo, &["init", "-q", "--initial-branch=main"]);
    git(&repo, &["config", "user.email", "agent@example.com"]);
    git(&repo, &["config", "user.name", "Agent"]);
    std::fs::write(repo.join("README.md"), "# served over the socket\n").expect("write README");
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    let worktree_name = "wt-over-the-socket";
    let worktree = repo_tmp.path().join(worktree_name);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            worktree.to_str().expect("worktree path is utf-8"),
            "-b",
            "socket-branch",
        ],
    );

    let project_id = uuid::Uuid::new_v4().to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "tonic-adapter-round-trip".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: repo
                .canonicalize()
                .expect("canonical repo")
                .display()
                .to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: HashMap::new(),
        },
    )
    .expect("register the project");

    AProject {
        project_id,
        worktree_name: worktree_name.to_string(),
        _repo_tmp: repo_tmp,
    }
}

/// A unary method of the host adapter, answered by the implementation behind it rather than by the
/// adapter's own default. Wiring this delegation to another of the seven `HostService` methods
/// compiles; only the answer says which one ran.
#[tokio::test]
async fn answers_a_unary_host_call_from_the_implementation_behind_the_adapter() {
    // Given
    let served = a_served_socket();
    let mut client = HostServiceClient::new(connect_channel(&served.socket_path).await);

    // When
    let response = client
        .list_eligible_daemons(ListEligibleDaemonsRequest {
            session_token: tddy_host_service::test_util::TEST_TOKEN.to_string(),
        })
        .await
        .expect("ListEligibleDaemons over the socket")
        .into_inner();

    // Then — this daemon lists itself, labelled the way a daemon labels its own row
    let local = response
        .daemons
        .iter()
        .find(|entry| entry.is_local)
        .unwrap_or_else(|| panic!("no local daemon in {:?}", response.daemons));
    assert!(
        local.label.ends_with("(this daemon)"),
        "the local row came back as {:?}, which is not this daemon's own label",
        local.label
    );
}

/// The refusal a host method owes its caller, which the adapter must convert rather than swallow:
/// `tddy_rpc::Status` and `tonic::Status` are different types, and a delegation that lost the code
/// on the way out would surface as `Unknown`.
#[tokio::test]
async fn carries_a_host_refusal_out_as_the_status_the_implementation_raised() {
    // Given
    let served = a_served_socket();
    let mut client = HostServiceClient::new(connect_channel(&served.socket_path).await);

    // When an unauthenticated caller asks
    let status = client
        .list_eligible_daemons(ListEligibleDaemonsRequest {
            session_token: "not-a-session".to_string(),
        })
        .await
        .expect_err("an unknown token must be refused");

    // Then
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

/// The host adapter's server-streaming arm. `StreamHostStats` emits once on connect before any
/// timer fires, so a frame arriving proves the adapter handed back the *implementation's* stream
/// and pumped it, not an empty one it built itself.
#[tokio::test]
async fn streams_host_stats_frames_the_implementation_produced() {
    // Given
    let served = a_served_socket();
    let mut client = HostServiceClient::new(connect_channel(&served.socket_path).await);

    // When
    let mut stream = client
        .stream_host_stats(StreamHostStatsRequest {
            session_token: tddy_host_service::test_util::TEST_TOKEN.to_string(),
        })
        .await
        .expect("StreamHostStats over the socket")
        .into_inner();
    let event = tokio::time::timeout(A_FRAME_ARRIVES_WITHIN, stream.message())
        .await
        .expect("no host-stats frame arrived within the timeout")
        .expect("the host-stats stream failed")
        .expect("the host-stats stream closed before its first frame");

    // Then — a real reading of this machine, not a default-valued message
    let cpu = event.cpu.expect("the immediate emit carries a CPU reading");
    assert!(
        cpu.logical_cores >= 1,
        "a host with {} logical cores is not a machine this daemon is running on",
        cpu.logical_cores
    );
}

/// The refusal on the streaming arm specifically: the adapter awaits the implementation's response
/// *before* it can wrap a stream, and that early error takes a different path out of the method
/// than a unary one.
#[tokio::test]
async fn refuses_a_host_stream_before_opening_it_when_the_caller_is_unknown() {
    // Given
    let served = a_served_socket();
    let mut client = HostServiceClient::new(connect_channel(&served.socket_path).await);

    // When
    let status = client
        .stream_host_stats(StreamHostStatsRequest {
            session_token: "not-a-session".to_string(),
        })
        .await
        .expect_err("an unknown token must be refused before the stream opens");

    // Then
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

/// A unary method of the worktree adapter, answered from the project the served implementation
/// reads off disk. The request body has to reach the handler intact for this to resolve at all —
/// the project id is what it is looked up by.
#[tokio::test]
async fn answers_a_unary_worktree_call_about_the_project_the_request_named() {
    // Given a registered project under the socket's data dir
    let served = a_served_socket();
    let project = a_project_under(served.dir.path());
    let mut client = WorktreeServiceClient::new(connect_channel(&served.socket_path).await);

    // When
    let response = client
        .list_worktrees_for_project(ListWorktreesForProjectRequest {
            session_token: tddy_worktree_service::test_util::TEST_TOKEN.to_string(),
            project_id: project.project_id.clone(),
            refresh: true,
        })
        .await
        .expect("ListWorktreesForProject over the socket")
        .into_inner();

    // Then — the project's own secondary worktree comes back
    assert!(
        names_the_worktree(&response.worktrees, &project.worktree_name),
        "the listing did not name `{}`: {:?}",
        project.worktree_name,
        paths_of(&response.worktrees)
    );
}

/// The worktree adapter's server-streaming arm. `StreamWorktreeStats` sends its snapshot frame
/// before any size walk finishes, so the first frame is the implementation's and carries the
/// project's worktrees.
#[tokio::test]
async fn streams_the_worktree_snapshot_frame_the_implementation_produced() {
    // Given
    let served = a_served_socket();
    let project = a_project_under(served.dir.path());
    let mut client = WorktreeServiceClient::new(connect_channel(&served.socket_path).await);

    // When
    let mut stream = client
        .stream_worktree_stats(StreamWorktreeStatsRequest {
            session_token: tddy_worktree_service::test_util::TEST_TOKEN.to_string(),
            project_id: project.project_id.clone(),
            recalculate_all: false,
        })
        .await
        .expect("StreamWorktreeStats over the socket")
        .into_inner();
    let event = tokio::time::timeout(A_FRAME_ARRIVES_WITHIN, stream.message())
        .await
        .expect("no worktree-stats frame arrived within the timeout")
        .expect("the worktree-stats stream failed")
        .expect("the worktree-stats stream closed before its first frame");

    // Then
    assert!(
        names_the_worktree(&event.snapshot, &project.worktree_name),
        "the snapshot frame did not name `{}`: {:?}",
        project.worktree_name,
        paths_of(&event.snapshot)
    );
}

/// The worktree streaming arm's refusal, for the same reason the host one is asserted: it leaves
/// the method before a stream exists to map.
#[tokio::test]
async fn refuses_a_worktree_stream_before_opening_it_when_the_caller_is_unknown() {
    // Given
    let served = a_served_socket();
    let project = a_project_under(served.dir.path());
    let mut client = WorktreeServiceClient::new(connect_channel(&served.socket_path).await);

    // When
    let status = client
        .stream_worktree_stats(StreamWorktreeStatsRequest {
            session_token: "not-a-session".to_string(),
            project_id: project.project_id.clone(),
            recalculate_all: false,
        })
        .await
        .expect_err("an unknown token must be refused before the stream opens");

    // Then
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

fn names_the_worktree(rows: &[WorktreeRow], worktree_name: &str) -> bool {
    rows.iter().any(|row| row.path.contains(worktree_name))
}

fn paths_of(rows: &[WorktreeRow]) -> Vec<&str> {
    rows.iter().map(|row| row.path.as_str()).collect()
}

// ---------------------------------------------------------------------------
// terminal_session.TerminalSessionService, the coordinate the jail dials
// ---------------------------------------------------------------------------

async fn a_terminal_client(socket_path: &Path) -> TerminalSessionServiceClient<Channel> {
    TerminalSessionServiceClient::new(connect_channel(socket_path).await)
}

/// The frame `tddy-sandbox-app`'s bridge opens its stream with: the token/session pair it
/// authenticates on, plus the initial in-band OSC resize that sizes the jailed PTY. No control
/// token, because an unclaimed session has no controlling screen to displace.
fn an_opening_terminal_frame(session_id: &str) -> SessionTerminalInput {
    SessionTerminalInput {
        session_token: TEST_TOKEN.to_string(),
        session_id: session_id.to_string(),
        data: b"\x1b]resize;100;30\x07".to_vec(),
        ..Default::default()
    }
}

/// The bidi method the in-jail app dials — the only bidirectional one in the surface, and the only
/// way a sandboxed session's terminal reaches its user. Naming a session this daemon is not running
/// is refused NOT_FOUND *by the implementation*, which is the assertion: the coordinate answers on
/// this socket. Without the mount the same call comes back UNIMPLEMENTED and every jail loses its
/// terminal.
#[tokio::test]
async fn opens_the_bidi_terminal_stream_the_in_jail_bridge_dials_over_this_socket() {
    // Given
    let served = a_served_socket();
    let mut client = a_terminal_client(&served.socket_path).await;

    // When the bridge's opening frame names a session with no running terminal
    let status = client
        .stream_session_terminal_io(tokio_stream::iter(vec![an_opening_terminal_frame(
            "no-such-session",
        )]))
        .await
        .expect_err("a session with no running terminal cannot open a terminal stream");

    // Then the terminal is what is missing — not the service
    assert_eq!(
        status.code(),
        tonic::Code::NotFound,
        "the terminal coordinate answered {:?} ({}) — UNIMPLEMENTED means it is not mounted on \
         this socket at all",
        status.code(),
        status.message()
    );
}

/// A unary terminal method, answered out of the daemon's own control lease rather than by a default
/// the adapter could have built: claiming issues a token, and only the lease behind the mounted
/// implementation can mint one.
#[tokio::test]
async fn grants_terminal_control_from_the_lease_behind_the_mounted_coordinate() {
    // Given
    let served = a_served_socket();
    let mut client = a_terminal_client(&served.socket_path).await;

    // When a screen claims control of an unheld session
    let response = client
        .claim_terminal_control(ClaimTerminalControlRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: "session-over-the-socket".to_string(),
            screen_id: "screen-a".to_string(),
            steal: false,
        })
        .await
        .expect("ClaimTerminalControl over the socket")
        .into_inner();

    // Then it holds the lease, with a token to present on later control calls
    assert!(response.granted, "an unheld lease must be granted");
    assert!(
        !response.control_token.is_empty(),
        "a granted claim without a control token is not a lease this screen can use"
    );
}

// ---------------------------------------------------------------------------
// session_agents.SessionAgentService and activity.ActivityService — the two
// coordinates `#unbundle` node 7 moved off `connection.ConnectionService`
//
// Both are mounted on this socket by `start_local_socket_server` above, for the reason the
// policy gives: `connection.ConnectionService` carried all 90 methods here, so dropping a family
// is a silent capability removal on a privileged interface — and for family B it is the jail's
// only transport, since five of `tddy-sandbox-runner`'s relay allowlist entries are its methods.
//
// Asserted through a call over the wire rather than by reading the builder: a coordinate left off
// it still answers over LiveKit and HTTP, and nowhere a jail can reach. An unmounted service
// comes back UNIMPLEMENTED, which is what distinguishes "mounted" from "declared".
// ---------------------------------------------------------------------------

/// A session directory under the served socket's sessions base, carrying the `.session.yaml` every
/// roster call resolves before it answers.
fn a_session_under(data_dir: &Path, session_id: &str) {
    let session_dir = tddy_core::session_lifecycle::unified_session_dir_path(
        &sessions_base_under(data_dir),
        session_id,
    );
    std::fs::create_dir_all(&session_dir).expect("create the session dir");
    tddy_core::write_initial_tool_session_metadata(
        &session_dir,
        tddy_core::InitialToolSessionMetadataOpts {
            project_id: "project-over-the-socket".to_string(),
            ..Default::default()
        },
    )
    .expect("write the session metadata");
}

/// A family-B unary method, answered by the roster store behind the generated adapter. The roster
/// echoes the session id it was asked about, so the request body had to reach the implementation
/// intact for this to hold — an adapter answering from a default would not know the name.
#[tokio::test]
async fn answers_a_unary_session_agent_call_from_the_implementation_behind_the_adapter() {
    // Given a session this daemon holds, so the roster has a directory to answer from
    let served = a_served_socket();
    a_session_under(served.dir.path(), "roster-over-the-socket");
    let mut client = SessionAgentServiceClient::new(connect_channel(&served.socket_path).await);

    // When an authenticated caller lists a session with no agents attached
    let roster = client
        .list_session_agents(ListSessionAgentsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: "roster-over-the-socket".to_string(),
            daemon_instance_id: String::new(),
        })
        .await
        .expect("ListSessionAgents over the socket")
        .into_inner();

    // Then the roster is this session's, and empty rather than absent
    assert_eq!(roster.session_id, "roster-over-the-socket");
    assert!(
        roster.agents.is_empty(),
        "a session nothing attached to came back carrying {:?}",
        roster.agents
    );
}

/// The activity coordinate, reached with the method `tddy-tools`' `session-hook` posts on every
/// Claude Code hook. An unknown status is refused by `ActivityServiceImpl` itself, before it
/// resolves any path, and the refusal quotes the status string the request carried — so this says
/// both that the coordinate is mounted here and that the body reached the handler.
#[tokio::test]
async fn carries_an_activity_refusal_out_of_the_handler_behind_the_adapter() {
    // Given
    let served = a_served_socket();
    let mut client = ActivityServiceClient::new(connect_channel(&served.socket_path).await);

    // When a hook reports a status no session type raises
    let status = client
        .report_session_status(ReportSessionStatusRequest {
            session_id: "status-over-the-socket".to_string(),
            hook_token: "tok-over-the-socket".to_string(),
            os_user: current_username(),
            status: "NotAStatusAnyHookRaises".to_string(),
        })
        .await
        .expect_err("an unknown activity status must be refused");

    // Then the implementation's own refusal came back, naming what it was sent
    assert_eq!(
        status.code(),
        tonic::Code::InvalidArgument,
        "the activity coordinate answered {:?} ({}) — UNIMPLEMENTED means it is not mounted on \
         this socket at all",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("NotAStatusAnyHookRaises"),
        "the refusal did not quote the status the request carried: {}",
        status.message()
    );
}
