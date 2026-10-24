//! Acceptance: an idle project-home session is stopped once it has been idle for
//! `project_home.idle_stop_secs`, keeps its conversation for the next `EnsureProjectHomeSession`,
//! and nothing else is ever auto-stopped.
//!
//! "Idle" is the hook-reported `activity_status` other than `Running`, since the metadata's
//! `updated_at` — which every status report bumps.
//!
//! Feature: docs/ft/daemon/project-concept.md § Project-home auto-stop (`#project-home` 6/7).

mod project_home_support;

use std::time::{Duration, SystemTime};

use project_home_support::{
    a_home_daemon, a_repo_with_origin_main, ensure_main, HomeDaemon, PROJECT_ID, VALID_TOKEN,
};
use tddy_rpc::Request;
use tddy_service::proto::session::{
    EnsureProjectHomeSessionResponse, ListSessionsRequest, SessionService as SessionServiceTrait,
    StartSessionRequest,
};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

const TEN_MINUTES: u64 = 600;

/// A daemon whose data dir holds one project (`origin/main` as its main branch ref), and a stub
/// `claude` that echoes its argv.
struct World {
    home: HomeDaemon,
    _repo: tempfile::TempDir,
}

impl World {
    /// Ensure main without a start spec: reach (or resume) the recorded session.
    async fn ensure(&self) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
        self.ensure_with(None).await
    }

    /// Set main up as a Claude CLI session, as the setup form does.
    async fn set_up_main(&self) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
        self.ensure_with(Some(StartSessionRequest {
            session_type: "claude-cli".to_string(),
            model: "opus".to_string(),
            ..Default::default()
        }))
        .await
    }

    /// Set main up and return its session id, running.
    async fn a_running_main(&self) -> String {
        self.set_up_main()
            .await
            .expect("set up main")
            .session
            .expect("main is returned")
            .session_id
    }

    async fn ensure_with(
        &self,
        start: Option<StartSessionRequest>,
    ) -> Result<EnsureProjectHomeSessionResponse, tddy_rpc::Status> {
        ensure_main(&self.home.service, start).await
    }
}

fn a_daemon_with_a_project(idle_stop_secs: u64) -> World {
    let repo = a_repo_with_origin_main();
    let project_home = format!("project_home:\n  idle_stop_secs: {idle_stop_secs}\n");
    World {
        home: a_home_daemon(repo.path(), "", &project_home),
        _repo: repo,
    }
}

impl World {
    /// Record a hook status report on `session_id`, now — as the `ReportSessionStatus` handler
    /// does (it also bumps `updated_at`).
    fn reported(&self, session_id: &str, activity_status: &str) {
        tddy_core::update_activity_status(
            &self.home.data.path().join("sessions").join(session_id),
            activity_status,
        )
        .expect("record the status report");
    }

    /// Run the reaper as if `later` had passed since now.
    async fn reap_after(&self, later: Duration) -> Vec<String> {
        self.home
            .service
            .reap_idle_project_home_sessions(SystemTime::now() + later)
            .await
    }

    async fn is_active(&self, session_id: &str) -> bool {
        self.home
            .service
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
            }))
            .await
            .expect("ListSessions")
            .into_inner()
            .sessions
            .into_iter()
            .find(|s| s.session_id == session_id)
            .expect("listed")
            .is_active
    }

    async fn an_ordinary_claude_session(&self) -> String {
        self.home
            .service
            .start_session(Request::direct(StartSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                project_id: PROJECT_ID.to_string(),
                session_type: "claude-cli".to_string(),
                model: "claude-opus-4-8".to_string(),
                ..Default::default()
            }))
            .await
            .expect("an ordinary session starts")
            .into_inner()
            .session_id
    }
}

const ELEVEN_MINUTES: Duration = Duration::from_secs(11 * 60);

#[tokio::test]
async fn an_idle_project_home_session_is_stopped_after_the_configured_time() {
    // Given a home session waiting for input for eleven minutes, with a ten-minute limit
    let world = a_daemon_with_a_project(TEN_MINUTES);
    let home = world.a_running_main().await;
    world.reported(&home, "WaitingForInput");
    let later = ELEVEN_MINUTES;

    // When
    let stopped = world.reap_after(later).await;

    // Then
    assert_eq!(stopped, vec![home.clone()]);
    assert!(!world.is_active(&home).await);
}

#[tokio::test]
async fn a_project_home_session_mid_turn_is_not_stopped() {
    // Given a home session whose turn has been running for eleven minutes
    let world = a_daemon_with_a_project(TEN_MINUTES);
    let home = world.a_running_main().await;
    world.reported(&home, "Running");
    let later = ELEVEN_MINUTES;

    // When
    let stopped = world.reap_after(later).await;

    // Then
    assert!(stopped.is_empty());
    assert!(world.is_active(&home).await);
}

#[tokio::test]
async fn ensuring_an_auto_stopped_home_session_resumes_the_same_conversation() {
    // Given a home session the reaper stopped
    let world = a_daemon_with_a_project(TEN_MINUTES);
    let home = world.a_running_main().await;
    world.reported(&home, "WaitingForInput");
    let later = ELEVEN_MINUTES;
    assert_eq!(world.reap_after(later).await, vec![home.clone()]);

    // When
    let resumed = world.ensure().await.expect("ensure resumes");

    // Then
    assert_eq!(resumed.outcome, "resumed");
    assert_eq!(resumed.session.unwrap().session_id, home);
    let handle = world
        .home
        .manager
        .get(&home)
        .await
        .expect("the agent runs again");
    let output = a_capture_showing(&handle.capture, "ARGV", PTY_STUB_OUTPUT).await;
    assert!(
        output.contains(&format!("--resume {home}")),
        "same conversation; got: {output:?}"
    );
}

#[tokio::test]
async fn an_ordinary_session_is_never_auto_stopped() {
    // Given an ordinary claude-cli session idle for eleven minutes
    let world = a_daemon_with_a_project(TEN_MINUTES);
    let ordinary = world.an_ordinary_claude_session().await;
    world.reported(&ordinary, "WaitingForInput");
    let later = ELEVEN_MINUTES;

    // When
    let stopped = world.reap_after(later).await;

    // Then
    assert!(stopped.is_empty());
    assert!(world.is_active(&ordinary).await);
}

#[tokio::test]
async fn idle_stop_secs_zero_disables_auto_stop() {
    // Given auto-stop switched off and a home session idle for a day
    let world = a_daemon_with_a_project(0);
    let home = world.a_running_main().await;
    world.reported(&home, "WaitingForInput");
    let later = Duration::from_secs(24 * 3600);

    // When
    let stopped = world.reap_after(later).await;

    // Then
    assert!(stopped.is_empty());
    assert!(world.is_active(&home).await);
}
