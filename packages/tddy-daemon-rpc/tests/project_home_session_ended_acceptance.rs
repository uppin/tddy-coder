//! Acceptance: an orchestrator's inbox tells it that a session of its project ended, and how
//! (`#activity-wake` 1/3).
//!
//! The session-ended notification is published on the host's bus as the daemon's agent-end
//! forwarder publishes it; the read is the real `ReadProjectNotifications` exec-tool call a
//! project-home agent makes, and the assertion is on the JSON it is answered with.
//!
//! Feature: docs/ft/daemon/session-notifications.md

mod support;

use std::sync::Arc;

use pretty_assertions::assert_eq;
use support::{
    a_host_logging_notifications, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT,
};
use tddy_daemon_kernel::session_notification::SessionEndNotice;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN, TEST_USER};

const READ_PROJECT_NOTIFICATIONS: &str = "ReadProjectNotifications";
/// A cursor before the first record of a log that never held one: everything is newer.
const FROM_THE_START: &str = "0";

struct World {
    a: Host,
    bus: Arc<SessionNotificationBus>,
    home: String,
    child: String,
    os_user: String,
}

/// One host holding a project-home session and a child session of the same project, with a
/// notification log on its bus.
async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let (a, bus) = a_host_logging_notifications(HOST_A, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let os_user = test_config()
        .os_user_for_github(TEST_USER)
        .expect("the test user maps to an OS user");
    World {
        a,
        bus,
        home,
        child,
        os_user,
    }
}

impl World {
    /// The notification the agent-end forwarder publishes when `os_user`'s child session's agent
    /// exits with `code`.
    fn the_childs_agent_exited_with(&self, os_user: &str, code: i32) -> SessionNotification {
        SessionNotification {
            session_id: self.child.clone(),
            os_user: os_user.to_string(),
            label: "fix-login".to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::SessionEnded,
            text: format!("⏹ Session fix-login ended: exit code {code}"),
            agent: None,
            question: None,
            ended: Some(SessionEndNotice {
                outcome: format!("exit code {code}"),
                exit_code: Some(code),
            }),
            at_unix_ms: 1_790_000_000_000,
            activity_status: None,
        }
    }

    /// The items of `ReadProjectNotifications` called by the home session, from the start.
    async fn the_home_reads_its_inbox(&self) -> Vec<serde_json::Value> {
        let answer = self
            .a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: READ_PROJECT_NOTIFICATIONS.to_string(),
                args_json: serde_json::json!({ "cursor": FROM_THE_START }).to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .expect("ReadProjectNotifications is served to the home session")
            .into_inner();
        assert!(
            !answer.is_error,
            "the read was refused: {}",
            answer.error_message
        );
        let page: serde_json::Value =
            serde_json::from_str(&answer.result_json).expect("the answer is JSON");
        page["items"].as_array().cloned().unwrap_or_default()
    }
}

#[tokio::test]
async fn the_inbox_returns_a_visible_sessions_end_with_its_outcome() {
    // Given a project-home session and a child of its project
    let world = a_world().await;

    // When the child's agent exits with code 1
    world
        .bus
        .publish(world.the_childs_agent_exited_with(&world.os_user, 1))
        .await;

    // Then the home session's inbox holds the end, its source and how it ended
    let items = world.the_home_reads_its_inbox().await;
    assert_eq!(
        items
            .iter()
            .map(|item| (
                item["session_id"].clone(),
                item["source"].clone(),
                item["text"].clone(),
                item["ended"].clone(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            serde_json::json!(world.child),
            serde_json::json!("session_ended"),
            serde_json::json!("⏹ Session fix-login ended: exit code 1"),
            serde_json::json!({ "outcome": "exit code 1", "exit_code": 1 }),
        )]
    );
}

#[tokio::test]
async fn the_inbox_does_not_return_the_end_of_a_session_the_caller_cannot_see() {
    // Given a project-home session, and a session of the project owned by another OS user
    let world = a_world().await;

    // When that session's agent ends
    world
        .bus
        .publish(world.the_childs_agent_exited_with("someone-else", 1))
        .await;

    // Then the home session's inbox holds nothing
    assert_eq!(
        world.the_home_reads_its_inbox().await,
        Vec::<serde_json::Value>::new()
    );
}
