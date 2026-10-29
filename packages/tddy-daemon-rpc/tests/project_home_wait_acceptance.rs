//! Acceptance: a project-home agent waits on its project's notifications and is told, within
//! moments of publication, what a child session needs (`#home-inbox` 7/8).
//!
//! The daemon keeps a project notification log on its notification bus; the child's notification
//! is published on that bus as the daemon's publish sites do, and the wait is the real exec-tool
//! call a home agent makes. The wait is bounded on both ends: the tool's own timeout, and the
//! test's bound around the call, so a wait that never returns fails its test instead of hanging it.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools,
//! docs/ft/daemon/session-notifications.md.
//! Changeset: docs/dev/changesets/2026-10-29-home-inbox-home-wait.md

mod support;

use std::sync::Arc;
use std::time::Duration;

use support::{
    a_host_logging_notifications, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT,
};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN, TEST_USER};

const WAIT_PROJECT_NOTIFICATIONS: &str = "WaitProjectNotifications";
/// How long the wait is asked to hold: only a published record ends it inside the test's bound.
const WAIT_TIMEOUT_MS: u64 = 8_000;
/// How long a test lets the wait call run before it counts as hung.
const BOUND: Duration = Duration::from_secs(15);
/// How long after the wait begins the child's notification is published.
const PUBLISH_DELAY: Duration = Duration::from_millis(150);
/// A cursor before the first record of a log that never held one: everything is newer.
const FROM_THE_START: &str = "0";

struct World {
    a: Host,
    bus: Arc<SessionNotificationBus>,
    home: String,
    child: String,
    ordinary: String,
    os_user: String,
}

/// One host holding a project-home session, a child of the project and an ordinary session of the
/// same project, with a notification log on its bus.
async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let (a, bus) = a_host_logging_notifications(HOST_A, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let ordinary = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let os_user = test_config()
        .os_user_for_github(TEST_USER)
        .expect("the test user maps to an OS user");
    World {
        a,
        bus,
        home,
        child,
        ordinary,
        os_user,
    }
}

impl World {
    /// The attention notification a child's agent raises when it wants the operator.
    fn a_notification_from_the_child(&self, text: &str) -> SessionNotification {
        SessionNotification {
            session_id: self.child.clone(),
            os_user: self.os_user.clone(),
            label: "my-feature-branch".to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::ActivityStatus,
            text: text.to_string(),
            agent: None,
            question: None,
            at_unix_ms: 1_790_000_000_000,
        }
    }

    /// `WaitProjectNotifications` called by `caller` on host-a.
    async fn waits_as(
        &self,
        caller: &str,
        args: serde_json::Value,
    ) -> Result<ExecuteToolResponse, Status> {
        let call = self
            .a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: caller.to_string(),
                tool_name: WAIT_PROJECT_NOTIFICATIONS.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }));
        tokio::time::timeout(BOUND, call)
            .await
            .unwrap_or_else(|_| panic!("the wait did not return within {BOUND:?}"))
            .map(|response| response.into_inner())
    }
}

#[tokio::test]
async fn a_project_home_agent_waits_and_receives_a_childs_agent_notification() {
    // Given a project-home session of the project, and a child of it that is about to need it
    let world = Arc::new(a_world().await);

    // When the home agent begins waiting, and the child's notification is then published
    let waiting = {
        let world = Arc::clone(&world);
        tokio::spawn(async move {
            let args = serde_json::json!({
                "cursor": FROM_THE_START,
                "timeout_ms": WAIT_TIMEOUT_MS,
            });
            world.waits_as(&world.home, args).await
        })
    };
    tokio::time::sleep(PUBLISH_DELAY).await;
    world
        .bus
        .publish(world.a_notification_from_the_child("my-feature-branch needs your input"))
        .await;

    // Then the wait returns with that notification and a cursor to continue from, not timed out
    let answer = waiting
        .await
        .expect("the wait task ran to completion")
        .expect("WaitProjectNotifications is served to the home session");
    assert!(
        !answer.is_error,
        "the wait was refused: {}",
        answer.error_message
    );
    let page: serde_json::Value =
        serde_json::from_str(&answer.result_json).expect("the answer is JSON");
    let first = &page["items"][0];
    assert_eq!(
        (
            page["items"].as_array().map(Vec::len),
            first["session_id"].as_str(),
            first["text"].as_str(),
            page["timed_out"].as_bool().unwrap_or(false),
            page["next_cursor"].as_str().is_some_and(|c| !c.is_empty()),
        ),
        (
            Some(1),
            Some(world.child.as_str()),
            Some("my-feature-branch needs your input"),
            false,
            true,
        ),
        "the wait's answer: {page}"
    );
}

#[tokio::test]
async fn a_non_project_home_caller_is_refused_as_permission_denied() {
    // Given an ordinary session of the project, whose child has published a notification
    let world = a_world().await;
    world
        .bus
        .publish(
            world.a_notification_from_the_child("a notification an ordinary session must not read"),
        )
        .await;

    // When that ordinary session calls WaitProjectNotifications
    let refusal = world
        .waits_as(
            &world.ordinary,
            serde_json::json!({ "cursor": FROM_THE_START, "timeout_ms": 100 }),
        )
        .await
        .expect_err("an ordinary session is refused");

    // Then it is refused as permission denied
    assert_eq!(refusal.code(), Code::PermissionDenied);
}
