//! Acceptance: a project-home session holding a live session watch is not auto-stopped while it
//! idles — it is kept up to be woken by what it watches — and once its watch ends it is stopped as
//! before (`#activity-wake` 3/3).
//!
//! The reaper is the daemon's, over a real host; the watch is created in the host's store as the
//! `CreateSessionWatch` tool creates it.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

mod project_home_support;

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use project_home_support::{
    a_home_daemon_keeping_session_watches, a_repo_with_origin_main, current_os_user, ensure_main,
    HomeDaemon, PROJECT_ID,
};
use tddy_service::proto::session::StartSessionRequest;
use tddy_session_activity::project_notification_log::ProjectNotificationLog;
use tddy_session_activity::session_watches::{
    SessionWatchStore, WatchFilter, WatchKind, WatchOptions, WatchOwner,
};

const ELEVEN_MINUTES: Duration = Duration::from_secs(11 * 60);
const AN_HOUR: Duration = Duration::from_secs(3_600);

struct World {
    home: HomeDaemon,
    watches: Arc<SessionWatchStore>,
    log: Arc<ProjectNotificationLog>,
    _repo: tempfile::TempDir,
}

fn a_daemon_stopping_idle_mains_after_ten_minutes() -> World {
    let repo = a_repo_with_origin_main();
    let (home, watches, log) = a_home_daemon_keeping_session_watches(
        repo.path(),
        "",
        "project_home:\n  idle_stop_secs: 600\n",
    );
    World {
        home,
        watches,
        log,
        _repo: repo,
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

impl World {
    /// Main set up as a Claude CLI session, running, waiting for input.
    async fn an_idle_main(&self) -> String {
        let main = ensure_main(
            &self.home.service,
            Some(StartSessionRequest {
                session_type: "claude-cli".to_string(),
                model: "opus".to_string(),
                ..Default::default()
            }),
        )
        .await
        .expect("set up main")
        .session
        .expect("main is returned")
        .session_id;
        tddy_core::update_activity_status(
            &self.home.data.path().join("sessions").join(&main),
            "WaitingForInput",
        )
        .expect("record the status report");
        main
    }

    /// `main` watches the sessions it starts, for `ttl`, as `CreateSessionWatch` creates a watch.
    async fn watches_for(&self, main: &str, ttl: Duration) {
        self.watches
            .create(
                &WatchOwner {
                    project_id: PROJECT_ID.to_string(),
                    session_id: main.to_string(),
                    os_user: current_os_user(),
                },
                WatchFilter {
                    session_ids: Vec::new(),
                    started_by_me: true,
                    kinds: WatchKind::ALL.to_vec(),
                },
                WatchOptions::default(),
                ttl,
                &self.log,
                now_unix_ms(),
            )
            .await
            .expect("the watch is created");
    }

    async fn reap_after(&self, later: Duration) -> Vec<String> {
        self.home
            .service
            .reap_idle_project_home_sessions(SystemTime::now() + later)
            .await
    }
}

#[tokio::test]
async fn a_project_home_session_holding_a_live_watch_is_not_reaped() {
    // Given main idle past the limit, holding a watch that lives for another hour
    let world = a_daemon_stopping_idle_mains_after_ten_minutes();
    let main = world.an_idle_main().await;
    world.watches_for(&main, AN_HOUR + ELEVEN_MINUTES).await;

    // When the reaper runs eleven minutes on
    let stopped = world.reap_after(ELEVEN_MINUTES).await;

    // Then main is kept up to be woken
    assert_eq!(stopped, Vec::<String>::new());
}

#[tokio::test]
async fn once_its_watch_ends_it_is_reaped_as_before() {
    // Given main idle past the limit, holding a watch that has expired by then
    let world = a_daemon_stopping_idle_mains_after_ten_minutes();
    let main = world.an_idle_main().await;
    world.watches_for(&main, Duration::from_secs(60)).await;

    // When the reaper runs eleven minutes on
    let stopped = world.reap_after(ELEVEN_MINUTES).await;

    // Then main is stopped as any idle main is
    assert_eq!(stopped, vec![main]);
}
