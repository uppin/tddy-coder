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
    a_caller_token, a_home_daemon, a_repo_with_origin_main, ensure_main, HomeDaemon, PROJECT_ID,
};
use tddy_rpc::Request;
use tddy_service::proto::activity::{ActivityService as _, ReportSessionStatusRequest};
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
                session_token: a_caller_token().to_string(),
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
                session_token: a_caller_token().to_string(),
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

impl World {
    /// An orchestrator session a person started (`#orchestrators` 1/11): main's placement and
    /// reach, but not main.
    async fn an_orchestrator_session(&self) -> String {
        self.home
            .service
            .start_session(Request::direct(StartSessionRequest {
                session_token: a_caller_token().to_string(),
                project_id: PROJECT_ID.to_string(),
                session_type: "claude-cli".to_string(),
                model: "opus".to_string(),
                session_role: "orchestrator".to_string(),
                ..Default::default()
            }))
            .await
            .expect("a person starts an orchestrator")
            .into_inner()
            .session_id
    }
}

const ELEVEN_MINUTES: Duration = Duration::from_secs(11 * 60);

/// The conversation main's agent runs after `/clear` (`#orchestrators` 6/11).
const THE_POST_CLEAR_CONVERSATION: &str = "6c1e0a00-0000-4000-8000-0000000006d1";

impl World {
    /// Main's agent ran `/clear` in the conversation it started with: its `SessionEnd` hook reports
    /// that conversation ended and its `SessionStart` hook the fresh one, as `tddy-tools
    /// session-hook` sends them, to this daemon's `ReportSessionStatus`.
    async fn cleared_into(&self, session_id: &str, conversation: &str) {
        let hook_token = tddy_core::read_session_metadata(
            &self.home.data.path().join("sessions").join(session_id),
        )
        .expect("main's .session.yaml")
        .hook_token
        .expect("main's hooks carry a token");
        let report = |status: &str, event: &str, (field, value): (&str, &str), id: &str| {
            ReportSessionStatusRequest {
                session_id: session_id.to_string(),
                hook_token: hook_token.clone(),
                os_user: "dev".to_string(),
                status: status.to_string(),
                hook_event_name: event.to_string(),
                agent_conversation_id: id.to_string(),
                hook_fields: [
                    (field.to_string(), value.to_string()),
                    ("session_id".to_string(), id.to_string()),
                ]
                .into_iter()
                .collect(),
                ..Default::default()
            }
        };
        for request in [
            report("Ended", "SessionEnd", ("reason", "clear"), session_id),
            report("Started", "SessionStart", ("source", "clear"), conversation),
        ] {
            self.home
                .service
                .activity_service()
                .report_session_status(Request::direct(request))
                .await
                .expect("the clear is reported");
        }
    }
}

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
async fn main_auto_stopped_after_a_clear_resumes_the_post_clear_conversation() {
    // Given main, whose agent ran `/clear`, stopped by the reaper once idle
    let world = a_daemon_with_a_project(TEN_MINUTES);
    let main = world.a_running_main().await;
    world.cleared_into(&main, THE_POST_CLEAR_CONVERSATION).await;
    world.reported(&main, "WaitingForInput");
    assert_eq!(world.reap_after(ELEVEN_MINUTES).await, vec![main.clone()]);

    // When
    let resumed = world.ensure().await.expect("ensure resumes");

    // Then main comes back in the conversation after the clear, not the one it started with
    assert_eq!(resumed.outcome, "resumed");
    let handle = world
        .home
        .manager
        .get(&main)
        .await
        .expect("the agent runs again");
    let output = a_capture_showing(&handle.capture, "ARGV", PTY_STUB_OUTPUT).await;
    assert!(
        output.contains(&format!("--resume {THE_POST_CLEAR_CONVERSATION}")),
        "the post-clear conversation; got: {output:?}"
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

#[tokio::test]
async fn idle_auto_stop_leaves_an_orchestrator_running() {
    // Given main and an orchestrator, both waiting for input for eleven minutes
    let world = a_daemon_with_a_project(TEN_MINUTES);
    let main = world.a_running_main().await;
    let orchestrator = world.an_orchestrator_session().await;
    world.reported(&main, "WaitingForInput");
    world.reported(&orchestrator, "WaitingForInput");
    let later = ELEVEN_MINUTES;

    // When
    let stopped = world.reap_after(later).await;

    // Then main alone is stopped: auto-stop is main's, not every orchestrator's
    assert_eq!(stopped, vec![main]);
    assert!(world.is_active(&orchestrator).await);
}
