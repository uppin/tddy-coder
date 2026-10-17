//! What the LiveKit plugin reports about itself on the integration status board — PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-integration.md` § "Integration status is
//! observable".
//!
//! The plugin is driven here through its own surface, `LiveKitSessionReach::open_now`, against a
//! host stand-in that holds one real git checkout: the session host is `tddy-session-lifecycle`'s,
//! and this crate may not depend on it. What is under test is what an open *reports*, so the host
//! serves no RPC and has no terminal to expose.
//!
//! The reachable case needs the LiveKit testkit (Docker, or `LIVEKIT_TESTKIT_WS_URL`) and is
//! `#[serial(livekit_docker)]`; the others dial nothing that answers.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use serial_test::serial;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_extensions::{
    IntegrationState, IntegrationStatusBoard, ReachableSessions, ReachedSession,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::session_room::{session_room_name, SessionRoomRegistry};
use tddy_daemon_livekit::{LiveKitSessionReach, LIVEKIT_INTEGRATION_ID};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::{MultiRpcService, RequestTransport, Status};

const INSTANCE_ID: &str = "livekit-integration-status-host";
const LK_API_KEY: &str = "devkey";
const LK_API_SECRET: &str = "secret";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The `livekit:` block of a daemon that hosts session rooms on `ws_url`.
fn a_livekit_block_at(ws_url: &str) -> String {
    format!(
        "livekit:\n  enabled: true\n  url: {ws_url}\n  api_key: {LK_API_KEY}\n  \
         api_secret: {LK_API_SECRET}\n  common_room: livekit-integration-status-lobby\n"
    )
}

/// A daemon configuration whose only LiveKit-relevant part is `livekit_block` (possibly empty).
fn a_daemon_config(livekit_block: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("daemon.yaml");
    std::fs::write(
        &path,
        format!("daemon_instance_id: {INSTANCE_ID}\n{livekit_block}"),
    )
    .expect("write daemon.yaml");
    let config = DaemonConfig::load(&path).expect("daemon.yaml must load");
    (dir, config)
}

/// A LiveKit address nothing listens on: bound, then released, so a dial is refused at once.
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

/// The session host, reduced to one session whose checkout is a real git repository here.
struct OneCheckoutOnThisHost {
    session: ReachedSession,
    /// What this host cannot do for the session, if anything.
    cannot: Option<HostCannot>,
    _home: tempfile::TempDir,
}

/// The part of serving a session the host stand-in refuses.
#[derive(Clone, Copy)]
enum HostCannot {
    BuildTheRoomServices,
    ExposeTheTerminal,
}

#[async_trait::async_trait]
impl ReachableSessions for OneCheckoutOnThisHost {
    async fn resolve(
        &self,
        _session_token: &str,
        _session_id: &str,
    ) -> Result<ReachedSession, Status> {
        Ok(self.session.clone())
    }

    fn session_services(&self) -> Result<MultiRpcService, Status> {
        match self.cannot {
            Some(HostCannot::BuildTheRoomServices) => Err(Status::internal(
                "the host cannot build this session's room services",
            )),
            _ => Ok(MultiRpcService::new(Vec::new())),
        }
    }

    async fn expose_terminal(&self, _session_id: &str) -> Result<(), Status> {
        match self.cannot {
            Some(HostCannot::ExposeTheTerminal) => Err(Status::internal(
                "the host cannot expose this session's terminal",
            )),
            _ => Ok(()),
        }
    }
}

/// A session whose checkout — one committed file — lives in a temp directory, reached from this
/// host.
fn a_session_with_a_checkout(suffix: &str) -> Arc<OneCheckoutOnThisHost> {
    a_session_the_host_cannot(suffix, None)
}

/// [`a_session_with_a_checkout`], on a host that refuses `cannot`.
fn a_session_the_host_cannot(
    suffix: &str,
    cannot: Option<HostCannot>,
) -> Arc<OneCheckoutOnThisHost> {
    let home = tempfile::tempdir().expect("tempdir");
    let session_id = format!("1780828020298-{suffix}");
    let worktree = home.path().join("checkout");
    a_checkout(&worktree);
    let session_dir = unified_session_dir_path(&home.path().join("sessions"), &session_id);
    std::fs::create_dir_all(&session_dir).expect("create the session directory");
    Arc::new(OneCheckoutOnThisHost {
        session: ReachedSession {
            session_id,
            session_dir,
            worktree_root: worktree,
            arrived_over: RequestTransport::InProcess,
        },
        cannot,
        _home: home,
    })
}

fn a_checkout(root: &Path) -> PathBuf {
    std::fs::create_dir_all(root).expect("create the checkout");
    git_ok(root, &["init", "--initial-branch=main"]);
    git_ok(root, &["config", "user.email", "agent@example.com"]);
    git_ok(root, &["config", "user.name", "Agent"]);
    std::fs::write(root.join("notes.md"), "one\n").expect("seed the checkout");
    git_ok(root, &["add", "."]);
    git_ok(root, &["commit", "-m", "seed"]);
    root.to_path_buf()
}

fn git_ok(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("git must be on PATH: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed in {cwd:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The plugin as the daemon wires it, reporting on a board of its own.
fn the_livekit_plugin(
    config: DaemonConfig,
) -> (Arc<LiveKitSessionReach>, Arc<IntegrationStatusBoard>) {
    let board = IntegrationStatusBoard::new();
    let reach = LiveKitSessionReach::new(
        config,
        Arc::new(SessionRoomRegistry::new()),
        board.reporter(LIVEKIT_INTEGRATION_ID, "LiveKit"),
    );
    (reach, board)
}

/// LiveKit's entry on `board`.
fn livekit_on(board: &IntegrationStatusBoard) -> IntegrationState {
    board
        .snapshot()
        .into_iter()
        .find(|integration| integration.id == LIVEKIT_INTEGRATION_ID)
        .expect("the plugin registers itself on the board")
        .state
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_room_that_cannot_be_opened_reports_livekit_unreachable_with_the_failure() {
    // Given the plugin configured with a LiveKit nothing listens on, and a session to open a room for
    let (_dir, config) = a_daemon_config(&a_livekit_block_at(
        &a_livekit_address_nothing_listens_on().await,
    ));
    let (reach, board) = the_livekit_plugin(config);
    let host = a_session_with_a_checkout("unreachable");

    // When it is asked to open the session's room now
    let refused = reach
        .open_now(&host.session, host.clone())
        .await
        .expect_err("a room LiveKit cannot create must not be reported as opened");

    // Then the board carries the very failure the caller was given
    assert_eq!(
        livekit_on(&board),
        IntegrationState::Unreachable {
            error: refused.message,
        }
    );
}

#[tokio::test]
async fn a_daemon_with_no_livekit_block_opens_no_room_and_reports_livekit_disabled() {
    // Given the plugin wired into a daemon whose configuration names no LiveKit at all
    let (_dir, config) = a_daemon_config("");
    let (reach, board) = the_livekit_plugin(config);
    let host = a_session_with_a_checkout("no-livekit");

    // When it is asked to open the session's room
    let opened = reach
        .open_now(&host.session, host.clone())
        .await
        .expect("a daemon with no LiveKit has nothing to fail at");

    // Then there is no room and nothing claims otherwise
    assert_eq!(opened, None);
    assert_eq!(livekit_on(&board), IntegrationState::Disabled);
}

#[tokio::test]
#[serial(livekit_docker)]
async fn a_room_opened_on_a_reachable_server_reports_livekit_connected() {
    // Given the plugin configured with a LiveKit that answers
    let testkit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker, or LIVEKIT_TESTKIT_WS_URL)");
    let (_dir, config) = a_daemon_config(&a_livekit_block_at(&testkit.get_ws_url()));
    let (reach, board) = the_livekit_plugin(config);
    let host = a_session_with_a_checkout(&uuid::Uuid::new_v4().simple().to_string());

    // When it opens the session's room
    let opened = reach
        .open_now(&host.session, host.clone())
        .await
        .expect("opening a room on a reachable server must succeed")
        .expect("a daemon configured with LiveKit hosts a room");

    // Then the room is the session's, and LiveKit is reported connected
    assert_eq!(opened.room, session_room_name(&host.session.session_id));
    assert_eq!(livekit_on(&board), IntegrationState::Connected);
}

#[tokio::test]
#[serial(livekit_docker)]
async fn a_terminal_that_cannot_be_exposed_fails_the_open_and_leaves_the_livekit_entry_alone() {
    // Given the plugin on a LiveKit that answers, for a session whose terminal the host cannot expose
    let testkit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker, or LIVEKIT_TESTKIT_WS_URL)");
    let (_dir, config) = a_daemon_config(&a_livekit_block_at(&testkit.get_ws_url()));
    let (reach, board) = the_livekit_plugin(config);
    let host = a_session_the_host_cannot(
        &uuid::Uuid::new_v4().simple().to_string(),
        Some(HostCannot::ExposeTheTerminal),
    );
    let before = livekit_on(&board);

    // When it opens the session's room
    reach
        .open_now(&host.session, host.clone())
        .await
        .expect_err("a terminal that cannot be exposed must still fail the request");

    // Then the failure was the session's, not the server's: the entry says what it said before
    assert_eq!(livekit_on(&board), before);
}

#[tokio::test]
async fn a_host_that_cannot_build_the_room_services_fails_the_open_and_leaves_the_livekit_entry_alone(
) {
    // Given the plugin configured with a LiveKit nothing listens on, for a session whose room
    // services the host cannot build, which is asked for before anything is dialled
    let (_dir, config) = a_daemon_config(&a_livekit_block_at(
        &a_livekit_address_nothing_listens_on().await,
    ));
    let (reach, board) = the_livekit_plugin(config);
    let host = a_session_the_host_cannot("no-services", Some(HostCannot::BuildTheRoomServices));
    let before = livekit_on(&board);

    // When it opens the session's room
    reach
        .open_now(&host.session, host.clone())
        .await
        .expect_err("a room whose services cannot be built must not open");

    // Then no server was reached or failed to be: the entry says what it said before
    assert_eq!(livekit_on(&board), before);
}
